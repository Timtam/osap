//! XInput, through the `rusty-xinput` crate.
//!
//! **Not linked.** windows-sys can import `XInputGetState` for us, and that import would be a
//! LOAD-TIME dependency on `xinput1_4.dll`: a machine without it (Server Core, a stripped
//! image) would then refuse to start the whole application, keyboard overlays and all, for the
//! sake of a feature nobody there uses. `rusty-xinput` loads the DLL at run time instead — the
//! loader under gilrs and Bevy — so a missing XInput is a feature that is simply absent. Its
//! handle is never freed, which is what a library used for the life of the process wants: a
//! library freed under a function pointer is how a crash at exit happens.
//!
//! **Ordinal 100.** `xinput1_4.dll` exports an unnamed `XInputGetStateEx` at ordinal 100 that
//! also reports the Guide button (bit 0x0400), and SDL has read it for years. The crate reads it
//! into a plain `XINPUT_STATE`, which is right: the `dwPaddingReserved` some headers add after
//! it is never written by Microsoft's DLLs (Wine found this when writing it broke programs that
//! had not allocated it). It is optional: where it is missing the pad simply has no `guide`.
//!
//! **Vendor and product.** The crate also reads the undocumented `XInputGetCapabilitiesEx`,
//! which gives the USB vendor and product id of the pad in a slot. Also optional: where it is
//! missing, `vendor` and `product` stay nil.

use rusty_xinput::{XInputHandle, XInputUsageError};
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
    assert!(names::xinput::GUIDE == rusty_xinput::XINPUT_GAMEPAD_GUIDE);
    assert!(xc::XUSER_MAX_COUNT == SLOTS as u32);
};

/// XInput has four user slots, and that is the limit for Xbox-type pads on Windows.
pub const SLOTS: usize = 4;

/// What the thread keeps of one reading: the parts of `XINPUT_STATE` it uses.
#[derive(Default, Clone, Copy)]
pub struct StateEx {
    pub packet: u32,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// Left x, left y, right x, right y — XInput's order, y positive UP.
    pub thumbs: [i16; 4],
}

/// `ERROR_DEVICE_NOT_CONNECTED`: what an empty slot answers.
const NOT_CONNECTED: u32 = 1167;

pub struct XInput {
    handle: XInputHandle,
    guide: bool,
    pub dll: &'static str,
}

impl XInput {
    /// The newest XInput the machine has, in the crate's own order. It is asked one DLL at a
    /// time, rather than through `load_default`, only so the log can say which one answered.
    pub fn load() -> Result<XInput, String> {
        const DLLS: [&str; 5] =
            ["xinput1_4.dll", "xinput1_3.dll", "xinput1_2.dll", "xinput1_1.dll", "xinput9_1_0.dll"];
        for dll in DLLS {
            let Ok(handle) = XInputHandle::load(dll) else { continue };
            // Slot 0 answers whether the extended call exists: "not loaded" is the crate's word
            // for a DLL without ordinal 100; an empty slot or a pad means it is there.
            let guide = !matches!(handle.get_state_ex(0), Err(XInputUsageError::XInputNotLoaded));
            return Ok(XInput { handle, guide, dll });
        }
        Err(format!("none of {} could be loaded", DLLS.join(", ")))
    }

    /// Whether the Guide button can be read (ordinal 100 was found).
    pub fn guide(&self) -> bool {
        self.guide
    }

    /// One slot's state, or the error code — `ERROR_DEVICE_NOT_CONNECTED` (1167) for an
    /// empty slot, which is the common case and not a fault.
    pub fn read(&self, slot: usize) -> Result<StateEx, u32> {
        let got = if self.guide {
            self.handle.get_state_ex(slot as u32)
        } else {
            self.handle.get_state(slot as u32)
        };
        match got {
            Ok(s) => {
                let g = &s.raw.Gamepad;
                Ok(StateEx {
                    packet: s.raw.dwPacketNumber,
                    buttons: g.wButtons,
                    left_trigger: g.bLeftTrigger,
                    right_trigger: g.bRightTrigger,
                    thumbs: [g.sThumbLX, g.sThumbLY, g.sThumbRX, g.sThumbRY],
                })
            }
            Err(XInputUsageError::UnknownError(code)) => Err(code),
            Err(_) => Err(NOT_CONNECTED),
        }
    }

    /// The pad's USB vendor and product id, where the extended capabilities call exists and
    /// answers for this slot.
    pub fn ids(&self, slot: usize) -> Option<(u16, u16)> {
        let c = self.handle.get_capabilities_ex(slot as u32).ok()?;
        (c.vendor_id != 0 || c.product_id != 0).then_some((c.vendor_id, c.product_id))
    }
}

pub fn snapshot(s: &StateEx, guide: bool) -> Snapshot {
    names::xinput::snapshot(s.buttons, s.left_trigger, s.right_trigger, s.thumbs, guide)
}

/// What the hub is told about a pad in slot `slot`. XInput gives no name, so every one is an
/// Xbox controller, which is what XInput pads present themselves as anyway; the vendor and
/// product id come from the extended capabilities call where it answers.
pub fn desc(slot: usize, guide: bool, ids: Option<(u16, u16)>) -> PadDesc {
    PadDesc {
        id: format!("xinput:{slot}"),
        name: "Xbox controller".to_string(),
        family: Family::Xbox,
        source: Source::XInput,
        mapped: true,
        vendor: ids.map(|(v, _)| v),
        product: ids.map(|(_, p)| p),
        buttons: names::xinput::buttons(guide),
        axes: names::AXES.to_vec(),
    }
}
