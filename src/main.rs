#[macro_use] extern crate log;
extern crate env_logger;
extern crate oping;
extern crate time;

use oping::{Ping, PingResult};
use std::time::{Duration, Instant};
use std::thread;
use std::env;
use log::{LogRecord, LogLevelFilter};
use env_logger::LogBuilder;

fn main() {
    let format = |record: &LogRecord| {
        let t = time::now();
        format!("{},{:03} - {} - {}",
            time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
            t.tm_nsec / 1000_000,
            record.level(),
            record.args()
        )
    };

    let mut builder = LogBuilder::new();
    builder
        .format(format)
        .filter(None, LogLevelFilter::Info);

    if env::var("RUST_LOG").is_ok() {
       builder.parse(&env::var("RUST_LOG").unwrap());
    }

    builder.init().unwrap();

    info!("Running.");
    
    let uptime = Instant::now();
    let mut downtime = None;

    loop {
        let mut ping = Ping::new();
        ping.set_timeout(2.0);
        ping.add_host("8.8.8.8");
        ping.add_host("4.2.2.2");
        ping.add_host("208.67.222.222");

        let mut dropped = 0;
        
        let responses = ping.send().unwrap();

        for resp in responses {
            if resp.dropped > 0 {
                if downtime.is_none() {
                    debug!("No response from {}", resp.hostname);
                }
                dropped += 1;
            }
            else {
                debug!("Response from host {}: latency {} ms",
                    resp.hostname, resp.latency_ms);

                if resp.latency_ms > 100.0 {
                    warn!("High latency from host {}: {} ms", resp.hostname, resp.latency_ms);
                }
            }
        }

        if dropped == 3 && downtime.is_none() {
            error!("All pings failed: Internet is down.");
            downtime = Some(Instant::now());
        }
        else if downtime.is_some() && dropped != 3 {
            info!("Internet was down for {}", 
                format_duration(Instant::now()
                    .duration_since(downtime.unwrap())));
            downtime = None;
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn format_duration(dur: Duration) -> String {
    let mut total = dur.as_secs();

    let hours = total / (60 * 60);
    total -= hours * (60 * 60);
    let mins = total  / 60;
    total -= mins * 60;
    let secs = total;

    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}