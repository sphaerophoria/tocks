use tocks::Tocks;
use ui::QmlUi;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let tocks_event_channel = tokio::sync::mpsc::unbounded_channel();
    let ui_event_channel = tokio::sync::mpsc::unbounded_channel();

    let _ui = QmlUi::new(ui_event_channel.0, tocks_event_channel.1);
    let mut tocks = Tocks::new(ui_event_channel.1, tocks_event_channel.0);

    tocks.run().await
}
