//! macOS 26+: `cargo run -p gpui-box-macos --features font-kit --example window_toolbar -- --smoke`.
//! Omit `--smoke` for interactive review. Native tabs are deliberately disabled: their strip
//! obscures full-size custom content. This is not a screenshot baseline or radius measurement.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("window_toolbar requires macOS 26+");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    macos::run()
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::{Result, ensure};
    use cocoa::{
        base::{id, nil},
        foundation::{NSOperatingSystemVersion, NSPoint, NSRect},
    };
    use gpui::{
        App, Application, AsyncApp, Bounds, Context, MouseButton, TitlebarOptions, Window,
        WindowBounds, WindowHandle, WindowOptions, WindowToolbarStyle, div, point, prelude::*, px,
        rgb, size,
    };
    use gpui_macos::MacPlatform;
    use objc::{
        class, msg_send,
        runtime::{BOOL, YES},
        sel, sel_impl,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    struct Demo {
        label: &'static str,
        style: Option<WindowToolbarStyle>,
        clicks: usize,
    }

    impl Render for Demo {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let header = div()
                .id("header-click")
                .h(px(40.))
                .pl(px(112.))
                .bg(rgb(0x226b60))
                .child(format!("Click header: {}", self.clicks))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.clicks += 1;
                        cx.notify();
                    }),
                );
            let fullscreen = div()
                .id("fullscreen")
                .child("Toggle fullscreen")
                .on_click(|_, window, _| window.toggle_fullscreen());
            div()
                .id("toolbar-demo")
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(0x172331))
                .text_color(rgb(0xffffff))
                .child(header)
                .child(
                    div()
                        .id("corner-content")
                        .flex_1()
                        .p_2()
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(self.label)
                        .child(fullscreen)
                        .child("Bottom edge — review corner clipping"),
                )
        }
    }

    fn surface(window: &Window) -> id {
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window)
            .expect("live window handle")
            .as_raw()
        else {
            unreachable!("macOS window")
        };
        handle.ns_view.as_ptr().cast()
    }

    fn inspect(window: &Window, style: Option<WindowToolbarStyle>, fullscreen: bool) -> Result<()> {
        // All native objects are borrowed from a live GPUI window on the main thread.
        unsafe {
            let view = surface(window);
            let native: id = msg_send![view, window];
            let toolbar: id = msg_send![native, toolbar];
            ensure!(toolbar.is_null() == style.is_none(), "toolbar presence");
            if let Some(style) = style {
                let expected = match style {
                    WindowToolbarStyle::Unified => 3isize,
                    WindowToolbarStyle::UnifiedCompact => 4,
                };
                let selected: isize = msg_send![native, toolbarStyle];
                let items: id = msg_send![toolbar, items];
                let count: usize = msg_send![items, count];
                let visible: BOOL = msg_send![toolbar, isVisible];
                ensure!(selected == expected && count == 0, "toolbar style/items");
                ensure!((visible == YES) != fullscreen, "toolbar visibility");
            }
            let tabs: id = msg_send![native, tabbedWindows];
            ensure!(tabs.is_null(), "native tab bar must be disabled");
            let content: id = msg_send![native, contentView];
            let bounds: NSRect = msg_send![content, bounds];
            let frame: NSRect = msg_send![view, frame];
            let viewport = window.viewport_size();
            ensure!(
                frame.origin.x == bounds.origin.x
                    && frame.origin.y == bounds.origin.y
                    && frame.size.width == bounds.size.width
                    && frame.size.height == bounds.size.height,
                "surface/content bounds mismatch"
            );
            ensure!(
                (f64::from(viewport.width) - bounds.size.width).abs() < 1.
                    && (f64::from(viewport.height) - bounds.size.height).abs() < 1.,
                "stale GPUI viewport"
            );
            ensure!(window.is_fullscreen() == fullscreen, "fullscreen state");
            let transparent: BOOL = msg_send![native, titlebarAppearsTransparent];
            ensure!((transparent == YES) != fullscreen, "titlebar transparency");
            let flipped: BOOL = msg_send![view, isFlipped];
            let y = if flipped == YES {
                20.
            } else {
                bounds.size.height - 20.
            };
            let local = NSPoint::new(200., y);
            let root: id = msg_send![content, superview];
            let parent: id = msg_send![root, superview];
            let p: NSPoint = msg_send![view, convertPoint: local toView: parent];
            let hit: id = msg_send![root, hitTest: p];
            ensure!(hit == view, "native titlebar occludes GPUI click target");
            let p: NSPoint = msg_send![view, convertPoint: local toView: nil];
            let screen: NSPoint = msg_send![native, convertPointToScreen: p];
            let actual: isize = msg_send![class!(NSWindow), windowNumberAtPoint: screen belowWindowWithWindowNumber: 0isize];
            let expected: isize = msg_send![native, windowNumber];
            ensure!(actual == expected, "window/toolbar child intercepts input");
            let layout: NSRect = msg_send![native, contentLayoutRect];
            let coordinates = |r: NSRect| [r.origin.x, r.origin.y, r.size.width, r.size.height];
            println!(
                "{style:?} fullscreen={fullscreen}: surface={:?}, layout={:?}",
                coordinates(frame),
                coordinates(layout)
            );
        }
        Ok(())
    }

    struct Completion(id, Arc<AtomicBool>);
    impl Completion {
        fn new(window: &Window, entering: bool) -> Self {
            unsafe {
                let native: id = msg_send![surface(window), window];
                let name = if entering {
                    c"NSWindowDidEnterFullScreenNotification"
                } else {
                    c"NSWindowDidExitFullScreenNotification"
                };
                let name: id = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
                let done = Arc::new(AtomicBool::new(false));
                let signal = done.clone();
                let block =
                    block::ConcreteBlock::new(move |_: id| signal.store(true, Ordering::SeqCst))
                        .copy();
                let center: id = msg_send![class!(NSNotificationCenter), defaultCenter];
                let observer: id = msg_send![center, addObserverForName: name object: native queue: nil usingBlock: &*block];
                let observer: id = msg_send![observer, retain];
                Self(observer, done)
            }
        }
    }
    impl Drop for Completion {
        fn drop(&mut self) {
            unsafe {
                let center: id = msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () = msg_send![center, removeObserver: self.0];
                let _: () = msg_send![self.0, release];
            }
        }
    }

    fn simulated_failure(window: &Window, entering: bool) -> Result<()> {
        unsafe {
            let native: id = msg_send![surface(window), window];
            let delegate: id = msg_send![native, delegate];
            let selector = if entering {
                sel!(windowDidFailToEnterFullScreen:)
            } else {
                sel!(windowDidFailToExitFullScreen:)
            };
            let supported: BOOL = msg_send![delegate, respondsToSelector: selector];
            ensure!(
                supported == YES && window.is_fullscreen() != entering,
                "failure callback/state unavailable"
            );
            let name = if entering {
                c"NSWindowWillEnterFullScreenNotification"
            } else {
                c"NSWindowWillExitFullScreenNotification"
            };
            let name: id = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
            let notification: id =
                msg_send![class!(NSNotification), notificationWithName: name object: native];
            // Exercise delegate recovery only; no OS fullscreen failure is induced.
            if entering {
                let _: () = msg_send![delegate, windowWillEnterFullScreen: notification];
            } else {
                let _: () = msg_send![delegate, windowWillExitFullScreen: notification];
            }
            let toolbar: id = msg_send![native, toolbar];
            let visible = !toolbar.is_null() && {
                let value: BOOL = msg_send![toolbar, isVisible];
                value == YES
            };
            if entering {
                let _: () = msg_send![delegate, windowDidFailToEnterFullScreen: native];
            } else {
                let _: () = msg_send![delegate, windowDidFailToExitFullScreen: native];
            }
            ensure!(!visible, "toolbar visible during simulated transition");
            println!(
                "Simulated failed fullscreen transition: entering={entering} (not an OS failure)"
            );
        }
        Ok(())
    }

    async fn pause(cx: &AsyncApp, milliseconds: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(milliseconds))
            .await;
    }

    async fn smoke(windows: Vec<WindowHandle<Demo>>, cx: &mut AsyncApp) -> Result<()> {
        for handle in windows {
            handle.update(cx, |_, window, _| window.activate_window())?;
            pause(cx, 400).await;
            handle.update(cx, |demo, window, _| inspect(window, demo.style, false))??;

            handle.update(cx, |_, window, _| window.resize(size(px(520.), px(340.))))?;
            pause(cx, 300).await;
            handle.update(cx, |demo, window, _| {
                ensure!(
                    window.viewport_size() == size(px(520.), px(340.)),
                    "resize failed"
                );
                inspect(window, demo.style, false)?;
                simulated_failure(window, true)?;
                inspect(window, demo.style, false)
            })??;
            for entering in [true, false] {
                let completion = handle.update(cx, |_, window, _| {
                    let completion = Completion::new(window, entering);
                    window.toggle_fullscreen();
                    completion
                })?;
                let deadline = Instant::now() + Duration::from_secs(8);
                while !completion.1.load(Ordering::SeqCst) {
                    ensure!(
                        Instant::now() < deadline,
                        "fullscreen completion timed out (entering={entering})"
                    );
                    pause(cx, 50).await;
                }
                pause(cx, 250).await;
                handle.update(cx, |demo, window, _| {
                    inspect(window, demo.style, entering)?;
                    if entering {
                        simulated_failure(window, false)?;
                        inspect(window, demo.style, true)?;
                    }
                    Ok::<_, anyhow::Error>(())
                })??;
            }
        }
        Ok(())
    }

    pub fn run() -> Result<()> {
        let smoke_mode = std::env::args().skip(1).any(|arg| arg == "--smoke");
        unsafe {
            let process: id = msg_send![class!(NSProcessInfo), processInfo];
            let supported: BOOL = msg_send![process, isOperatingSystemAtLeastVersion: NSOperatingSystemVersion::new(26, 0, 0)];
            ensure!(supported == YES, "window_toolbar requires macOS 26+");
        }
        if smoke_mode {
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_secs(60));
                eprintln!("window_toolbar smoke exceeded 60 seconds");
                std::process::exit(124);
            });
        }
        Application::with_platform(Rc::new(MacPlatform::new(false))).run(move |cx: &mut App| {
            let mut windows = Vec::new();
            for (index, (label, style)) in [
                ("No toolbar", None),
                ("UnifiedCompact", Some(WindowToolbarStyle::UnifiedCompact)),
                ("Unified", Some(WindowToolbarStyle::Unified)),
            ]
            .into_iter()
            .enumerate()
            {
                let mut bounds = Bounds::centered(None, size(px(440.), px(300.)), cx);
                bounds.origin.x += px((index as f32 - 1.) * 450.);
                let handle = cx
                    .open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            titlebar: Some(TitlebarOptions {
                                title: Some(label.into()),
                                appears_transparent: true,
                                traffic_light_position: Some(point(px(16.), px(16.))),
                                toolbar_style: style,
                            }),
                            tabbing_identifier: None,
                            app_owns_titlebar_drag: true,
                            ..Default::default()
                        },
                        |_, cx| {
                            cx.new(|_| Demo {
                                label,
                                style,
                                clicks: 0,
                            })
                        },
                    )
                    .expect("open toolbar demo");
                windows.push(handle);
            }
            cx.activate(true);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            if smoke_mode {
                cx.spawn(async move |cx| {
                    if let Err(error) = smoke(windows, cx).await {
                        eprintln!("window_toolbar smoke FAILED: {error:#}");
                        std::process::exit(1);
                    }
                    println!("window_toolbar smoke PASSED");
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
        Ok(())
    }
}
