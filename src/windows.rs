pub fn run() {
    if let Ok(path) = std::env::current_exe() {
        let settings_path = match path.parent() {
            Some(parent) => parent.join("settings"),
            None => std::path::PathBuf::from("settings"),
        };

        let builder =
            Config::builder().add_source(config::File::with_name(settings_path.to_str().unwrap()));
        match builder.build() {
            Ok(config) => *SETTINGS.lock().unwrap() = config,
            Err(err) => warn!("settings merge failed, use default settings, err: {}", err),
        }
    }

    let api = {
        let settings = SETTINGS.lock().unwrap();
        settings
            .get_string("api")
            .unwrap_or("https://deepl.deno.dev/translate".to_string())
    };

    let (width, height) = {
        let settings = SETTINGS.lock().unwrap();
        (
            settings.get_float("window.size.width").unwrap_or(500.0) as f32,
            settings.get_float("window.size.height").unwrap_or(200.0) as f32,
        )
    };

    let (tx_hk, rx) = mpsc::channel();

    let mut hotkey_settings = HotkeySetting::default();
    hotkey_settings.register_hotkey(tx_hk.clone());

    // embed ico file
    let ioc_buf = Cursor::new(include_bytes!("../res/copy-translator.ico"));
    let icon_dir = ico::IconDir::read(ioc_buf).unwrap();
    let image = icon_dir.entries()[5].decode().unwrap();
    let ico_data = eframe::IconData {
        rgba: std::vec::Vec::from(image.rgba_data()),
        width: image.width(),
        height: image.height(),
    };

    // listen for global mouse event
    let (rdev_tx, rdev_rx) = mpsc::sync_channel(1);
    let mouse_event_rx_wrap = std::sync::Arc::new(std::sync::Mutex::new(rdev_rx));
    thread::spawn(move || {
        // let last_move =
        if let Err(error) = rdev::listen(move |event| {
            match event.event_type {
                rdev::EventType::ButtonPress(button) => {
                    if button == rdev::Button::Left {
                        let _ = rdev_tx.try_send(ui::Event::MouseEvent(event.event_type));
                    }
                }
                rdev::EventType::ButtonRelease(button) => {
                    if button == rdev::Button::Left {
                        let _ = rdev_tx.try_send(ui::Event::MouseEvent(event.event_type));
                    }
                }
                rdev::EventType::MouseMove { x: _, y: _ } => {
                    let _ = rdev_tx.try_send(ui::Event::MouseEvent(event.event_type));
                }
                _ => {}
            };
        }) {
            warn!("rdev listen error: {:?}", error)
        }
    });

    loop {
        match rx.recv() {
            Ok(text) => {
                let (event_tx, event_rx) = mpsc::sync_channel(1);
                let (task_tx, task_rx) = mpsc::sync_channel(1);

                let event_tx_trasnlate = event_tx.clone();
                let api = api.clone();

                thread::spawn(move || {
                    while let Ok((text, target_lang, source_lang)) = task_rx.recv() {
                        let _ = match deepl::translate(&api, text, target_lang, source_lang) {
                            Ok(text) => event_tx_trasnlate.send(ui::Event::TextSet(text)),
                            Err(_err) => event_tx_trasnlate
                                .send(ui::Event::TextSet("翻译接口失效，请更换".into())),
                        };
                    }
                });

                let mouse_event_rx = mouse_event_rx_wrap.clone();
                let event_tx_mouse = event_tx.clone();
                thread::spawn(move || {
                    loop {
                        let rx = mouse_event_rx.lock().unwrap();
                        match rx.recv() {
                            Ok(event) => {
                                if event_tx_mouse.send(event).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
                hotkey_settings.unregister_all();
                let native_options = eframe::NativeOptions {
                    always_on_top: true,
                    decorated: false,
                    initial_window_size: Some(egui::vec2(width, height)),
                    icon_data: Some(ico_data.clone()),
                    drag_and_drop_support: true,
                    ..Default::default()
                };
                thread::spawn(|| {
                    eframe::run_native(
                        "Copy Translator",
                        native_options,
                        Box::new(|cc| Box::new(ui::MyApp::new(text, event_rx, task_tx, cc))),
                    );
                });
                hotkey_settings.register_hotkey(tx_hk.clone());
            }
            Err(err) => {
                panic!("{}", err)
            }
        }
    }
}
