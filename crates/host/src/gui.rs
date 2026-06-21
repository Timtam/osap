//! Minimal wxDragon GUI host. wxWidgets owns the main message loop, so we hand
//! it our OS-event pump via a recurring [`Timer`] tick: global hotkeys, key
//! capture and window triggers keep firing while the window is open. This is
//! the event-loop coexistence proof (see `docs/architecture-feasibility-study.md`
//! §11.6). Windows-first; the pump itself lives behind the platform backend.

use wxdragon::prelude::*;
use wxdragon::timer::Timer;

/// Opens the host window and runs the wx main loop, calling `pump` on every
/// timer tick to drain OS events into the dispatcher. Blocks until the window
/// is closed.
pub fn run_gui(mut pump: impl FnMut() + 'static) -> Result<(), Box<dyn std::error::Error>> {
    wxdragon::main(move |_app| {
        let frame = Frame::builder()
            .with_title("Automation Platform")
            .with_size(Size::new(480, 240))
            .build();

        let panel = Panel::builder(&frame).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        let label = StaticText::builder(&panel)
            .with_label(
                "Automation Platform is running.\n\n\
                 Loaded modules are active. Global hotkeys, key capture and\n\
                 window triggers keep working while this window is open.\n\n\
                 Close this window to quit.",
            )
            .build();
        sizer.add(&label, 1, SizerFlag::All | SizerFlag::Expand, 16);
        panel.set_sizer(sizer, true);

        // wxWidgets owns the message loop now, so this recurring tick is how our
        // OS events (hotkeys / captured keys / foreground changes) get drained
        // into Luau. The init closure returns *before* the loop runs, so the
        // timer must outlive it — leak it for the app's lifetime (its Drop would
        // otherwise stop and destroy the underlying wxTimer).
        let timer = Timer::new(&frame);
        timer.on_tick(move |_event| pump());
        timer.start(15, false);
        std::mem::forget(timer);

        frame.show(true);
    })
}
