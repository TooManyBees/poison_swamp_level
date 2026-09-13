use hyper_util::server::graceful::GracefulShutdown;
use poison_swamp_level::service::AppConfig;
use poison_swamp_level::{Config, init_logger};
use std::io::{self, IsTerminal};
use std::time::Duration;
use tokio::time::sleep;
#[cfg(unix)]
use tokio::{
    signal::ctrl_c,
    signal::unix::{SignalKind, signal},
    sync::{mpsc, mpsc::Sender},
};

#[tokio::main(flavor = "local")]
async fn main() {
    let config_path = config_path().unwrap_or_else(|_| {
        eprintln!("{}", usage());
        std::process::exit(1);
    });
    let config = Config::read_from_file(&config_path).unwrap_or_else(|e| {
        if io::stderr().is_terminal() {
            eprintln!("{}", e.explain());
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    });

    init_logger(&config);

    log::info!("Startup");
    let mut app_config = AppConfig::from_config(config).unwrap_or_else(|e| {
        log::error!("Aborting program: {e}");
        std::process::exit(1);
    });

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

    // Signal handlers for reloading the config
    #[cfg(unix)]
    let mut sighup = signal(SignalKind::hangup()).expect("Failed to install SIGHUP handler");
    #[cfg(unix)]
    let (config_reload_tx, mut config_reload) = mpsc::channel::<AppConfig>(1);

    // Signal handlers for shutting down
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
    let mut shutdown_signal = std::pin::pin!(ctrl_c_handler());
    let graceful = GracefulShutdown::new();

    loop {
        #[cfg(not(unix))]
        tokio::select! {
            listen_result = listener.accept() => {
                app_config.handle_connection(listen_result, &graceful);
            },
            metrics_listen_result = metrics_listener.accept() => {
                app_config.handle_metrics(metrics_listen_result, &graceful);
            },
            _ = &mut shutdown_signal => break,
        }
        #[cfg(unix)]
        tokio::select! {
            listen_result = listener.accept() => {
                app_config.handle_connection(listen_result, &graceful);
            },
            metrics_listen_result = metrics_listener.accept() => {
                app_config.handle_metrics(metrics_listen_result, &graceful);
            },
            Some(mut new_app_config) = config_reload.recv() => {
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
                new_app_config.replace_metrics(&app_config);
                app_config = new_app_config;

                log::info!("Reloaded config");
            },
            _ = sighup.recv() => {
                reload_config(&config_path, &app_config, config_reload_tx.clone());
            },
            _ = sigterm.recv() => break,
            _ = &mut shutdown_signal => break,
        }
    }

    const GRACE_SECONDS: u64 = 5;
    tokio::select! {
        _ = graceful.shutdown() => {}
        _ = sleep(Duration::from_secs(GRACE_SECONDS)) => {
            log::warn!("Service terminated after waiting {GRACE_SECONDS} seconds");
        }
    }

    app_config.persist_metrics();
    log::info!("Shutdown");
}

fn config_path() -> Result<String, ()> {
    let mut args = std::env::args().skip(1);
    while args.len() > 0 {
        match args.next().unwrap().to_lowercase().as_str() {
            "-c" | "--config" => match args.next() {
                Some(c) => return Ok(c),
                None => return Err(()),
            },
            _arg => {}
        }
    }
    Ok("./config.kdl".into())
}

fn usage() -> String {
    "
Usage: poison_swamp_level [-c <path to config file>]

A config file in KDL format is required. If not given as an argument,
poison_swamp_level will try to open ./config.kdl in the current dir.

"
    .trim()
    .into()
}

async fn ctrl_c_handler() {
    ctrl_c()
        .await
        .expect("Failed to install SIGINT/Ctrl-C handler");
}

#[cfg(unix)]
fn reload_config(path: &str, existing: &AppConfig, tx: Sender<AppConfig>) {
    let new_config = match Config::read_from_file(path) {
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
