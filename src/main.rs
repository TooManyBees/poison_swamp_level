use poison_swamp_level::service::AppConfig;
use poison_swamp_level::{Config, init_logger};
use std::io::{self, IsTerminal};
#[cfg(unix)]
use tokio::{
    signal::unix::{SignalKind, signal},
    sync::{mpsc, mpsc::Sender},
};

#[tokio::main(flavor = "local")]
async fn main() {
    let config = match Config::read_from_file("./config.kdl") {
        Ok(config) => config,
        Err(e) => {
            if io::stderr().is_terminal() {
                eprintln!("{}", e.explain());
            } else {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    };

    init_logger(&config);

    let mut app_config = AppConfig::from_config(config).unwrap();
    #[allow(unused_mut)]
    let mut listener = app_config
        .listen()
        .await
        .unwrap_or_else(|_| std::process::exit(1));
    #[allow(unused_mut)]
    let mut metrics_listener = app_config
        .listen_metrics()
        .await
        .unwrap_or_else(|_| std::process::exit(1));

    #[cfg(unix)]
    let mut sighup = signal(SignalKind::hangup()).unwrap();
    #[cfg(unix)]
    let (config_reload_tx, mut config_reload) = mpsc::channel::<AppConfig>(1);

    loop {
        #[cfg(not(unix))]
        tokio::select! {
            listen_result = listener.accept() => {
                handle_connection(&app_config, listen_result);
            },
            metrics_listen_result = metrics_listener.accept() => {
                handle_metrics(&app_config, metrics_listen_result);
            },
        }
        #[cfg(unix)]
        tokio::select! {
            listen_result = listener.accept() => {
                app_config.handle_connection(listen_result);
            },
            metrics_listen_result = metrics_listener.accept() => {
                app_config.handle_metrics(metrics_listen_result);
            },
            Some(new_app_config) = config_reload.recv() => {
                let mut new_listener = None;
                if app_config.listen_addr() != new_app_config.listen_addr() {
                    match new_app_config.listen().await {
                        Ok(l) => new_listener = Some(l),
                        Err(e) => {
                            log::error!("Config reload encountered error: {e}");
                            continue;
                        }
                    }
                }
                let mut new_metrics_listener = None;
                if app_config.listen_metrics_addr() != new_app_config.listen_metrics_addr() {
                    match new_app_config.listen_metrics().await {
                        Ok(l) => new_metrics_listener = Some(l),
                        Err(e) => {
                            log::error!("Config reload encountered error: {e}");
                            continue;
                        }
                    }
                }

                if let Some(l) = new_listener {
                    listener = l;
                }
                if let Some(l) = new_metrics_listener {
                    metrics_listener = l;
                }
                app_config = new_app_config;

                log::info!("Reloaded config");
            },
            _ = sighup.recv() => {
                reload_config(&app_config, config_reload_tx.clone());
            },
        }
    }
}

#[cfg(unix)]
fn reload_config(existing: &AppConfig, tx: Sender<AppConfig>) {
    let new_config = match Config::read_from_file("./config.kdl") {
        Ok(config) => config,
        Err(e) => {
            log::error!("Config reload encountered error: {e}");
            return;
        }
    };

    if existing.same_as(&new_config) {
        log::info!("Reloaded config: no changes");
        return;
    }

    std::thread::spawn(move || match AppConfig::from_config(new_config) {
        Ok(app_config) => tx.blocking_send(app_config),
        Err(e) => Ok({
            log::error!("Config reload encountered error: {e}");
        }),
    });
}
