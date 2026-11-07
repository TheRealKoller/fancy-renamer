use anyhow::Result;
use gtk::prelude::*;

const APP_ID: &str = "io.github.therealKoller.FancyRenamer";

pub fn run() -> Result<()> {
    let app = libadwaita::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_window);

    // gtk::Application::run liefert einen Exit-Code; Argumente kommen von der Umgebung.
    app.run();
    Ok(())
}

fn build_window(app: &libadwaita::Application) {
    let header = libadwaita::HeaderBar::new();

    let empty_state = libadwaita::StatusPage::builder()
        .title("Keine Dateien")
        .description("Dateien hinzufügen, um sie umzubenennen.")
        .icon_name("folder-open-symbolic")
        .vexpand(true)
        .build();

    let toolbar_view = libadwaita::ToolbarView::builder()
        .content(&empty_state)
        .build();
    toolbar_view.add_top_bar(&header);

    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("fancy-renamer")
        .default_width(900)
        .default_height(650)
        .content(&toolbar_view)
        .build();

    window.present();
}
