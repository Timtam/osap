//! XInput, loaded at run time.
//!
//! **Not linked.** windows-sys can import `XInputGetState` for us, and that import would be a
//! LOAD-TIME dependency on `xinput1_4.dll`: a machine without it (Server Core, a stripped
//! image) would then refuse to start the whole application, keyboard overlays and all, for the
//! sake of a feature nobody there uses. `LoadLibraryW` makes it a feature that is simply
//! absent. Only windows-sys's constants are used, and those cost nothing at run time.
//!
//! **Ordinal 100.** `xinput1_4.dll` exports an unnamed `XInputGetStateEx` at ordinal 100 that
//! also reports the Guide button (bit 0x0400), and SDL has read it for years. It is optional:
//! where it is missing — `xinput9_1_0.dll` has no such export — the pad simply has no `guide`.

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Input::XboxController as xc;

use super::{names, Family, PadDesc, Snapshot, Source};

// names.rs writes the bits out so that its table compiles and is tested on every platform.
// This is where it is held to the header: a mismatch is a build failure, not a wrong button.
const _: () = {
    assert!(names::xinput::A == xc::XINPUT_GAMEPAD_A);
    assert!(names::xinput::B == xc::XINPUT_GAMEPAD_B);
    assert!(names::xinput::X == xc::XINPUT_GAMEPAD_X);
    assert!(names::xinput::Y == xc::XINPUT_GAMEPAD_Y);
    assert!(names::xinput::BACK == xc::XINPUT_GAMEPAD_BACK);
    assert!(names::xinput::START == xc::XINPUT_GAMEPAD_START);
    assert!(names::xinput::LEFT_THUMB == xc::XINPUT_GAMEPAD_LEFT_THUMB);
    assert!(names::xinput::RIGHT_THUMB == xc::XINPUT_GAMEPAD_RIGHT_THUMB);
    assert!(names::xinput::LEFT_SHOULDER == xc::XINPUT_GAMEPAD_LEFT_SHOULDER);
    assert!(names::xinput::RIGHT_SHOULDER == xc::XINPUT_GAMEPAD_RIGHT_SHOULDER);
    assert!(names::xinput::DPAD_UP == xc::XINPUT_GAMEPAD_DPAD_UP);
    assert!(names::xinput::DPAD_DOWN == xc::XINPUT_GAMEPAD_DPAD_DOWN);
    assert!(names::xinput::DPAD_LEFT == xc::XINPUT_GAMEPAD_DPAD_LEFT);
    assert!(names::xinput::DPAD_RIGHT == xc::XINPUT_GAMEPAD_DPAD_RIGHT);
    assert!(xc::XUSER_MAX_COUNT == SLOTS as u32);
    // The documented struct is the first 16 bytes of ours; the extended call writes 4 more.
    assert!(std::mem::size_of::<xc::XINPUT_STATE>() == 16);
    assert!(std::mem::size_of::<StateEx>() == 20);
};

/// XInput has four user slots, and that is the limit for Xbox-type pads on Windows.
pub const SLOTS: usize = 4;

/// `XINPUT_STATE` followed by the reserved DWORD that ordinal 100 writes (SDL's
/// `XINPUT_STATE_EX`). Used for BOTH calls: the documented one fills the first 16 bytes and
/// leaves the last four alone, so one buffer shape serves either function.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct StateEx {
    pub packet: u32,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// Left x, left y, right x, right y — XInput's order, y positive UP.
    pub thumbs: [i16; 4],
    pub reserved: u32,
}

type GetState = unsafe extern "system" fn(u32, *mut StateEx) -> u32;

pub struct XInput {
    /// The call that is actually made: ordinal 100 when there is one, `XInputGetState`
    /// otherwise.
    get_state: GetState,
    guide: bool,
    pub dll: &'static str,
}

impl XInput {
    /// The newest XInput the machine has. The module handle is never freed: the library is
    /// used for the life of the process, and freeing it under a function pointer is how a
    /// crash at exit happens.
    pub fn load() -> Result<XInput, String> {
        for dll in ["xinput1_4.dll", "xinput9_1_0.dll"] {
            let wide: Vec<u16> = dll.encode_utf16().chain(Some(0)).collect();
            // SAFETY: a NUL-terminated wide string that outlives the call.
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            if module.is_null() {
                continue;
            }
            // SAFETY: a NUL-terminated name, and a module that is loaded and stays loaded.
            let named = unsafe { GetProcAddress(module, b"XInputGetState\0".as_ptr()) };
            let Some(named) = named else { continue };
            // An ordinal is passed where the name would be, as MAKEINTRESOURCEA does.
            // SAFETY: GetProcAddress accepts an ordinal in the low word of the name pointer.
            let ordinal = unsafe { GetProcAddress(module, 100usize as *const u8) };
            let (f, guide) = match ordinal {
                Some(f) => (f, true),
                None => (named, false),
            };
            // SAFETY: both exports have the signature `DWORD (DWORD, XINPUT_STATE*)`; the
            // extended one writes 4 bytes more, which `StateEx` has room for.
            let get_state: GetState = unsafe { std::mem::transmute(f) };
            return Ok(XInput { get_state, guide, dll });
        }
        Err("neither xinput1_4.dll nor xinput9_1_0.dll could be loaded".to_string())
    }

    /// Whether the Guide button can be read (ordinal 100 was found).
    pub fn guide(&self) -> bool {
        self.guide
    }

    /// One slot's state, or the error code — `ERROR_DEVICE_NOT_CONNECTED` (1167) for an
    /// empty slot, which is the common case and not a fault.
    pub fn read(&self, slot: usize) -> Result<StateEx, u32> {
        let mut s = StateEx::default();
        // SAFETY: a valid out pointer to a buffer as large as either export writes.
        let r = unsafe { (self.get_state)(slot as u32, &mut s) };
        if r == 0 {
            Ok(s)
        } else {
            Err(r)
        }
    }
}

pub fn snapshot(s: &StateEx, guide: bool) -> Snapshot {
    names::xinput::snapshot(s.buttons, s.left_trigger, s.right_trigger, s.thumbs, guide)
}

/// What the hub is told about a pad in slot `slot`. XInput says nothing about which pad it
/// is — no name, no vendor or product id without another undocumented export — so every one
/// is an Xbox controller, which is what XInput pads present themselves as anyway.
pub fn desc(slot: usize, guide: bool) -> PadDesc {
    PadDesc {
        id: format!("xinput:{slot}"),
        name: "Xbox controller".to_string(),
        family: Family::Xbox,
        source: Source::XInput,
        mapped: true,
        vendor: None,
        product: None,
        buttons: names::xinput::buttons(guide),
        axes: names::AXES.to_vec(),
    }
}
