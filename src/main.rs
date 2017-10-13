extern crate chrono;
extern crate oping;
extern crate pancurses;
extern crate time;

use chrono::prelude::*;
use oping::{Ping, PingItem};
use std::time::Duration;
use std::thread;
use std::sync::mpsc;
use pancurses::*;

const COLOR_TABLE: [i16; 8] = [COLOR_RED,
                                COLOR_BLUE,
                                COLOR_GREEN,
                                COLOR_CYAN,
                                COLOR_RED,
                                COLOR_MAGENTA,
                                COLOR_YELLOW,
                                COLOR_WHITE];

#[derive(Copy, Clone, Debug)]
struct Period {
    start: DateTime<Local>,
    len: Duration,
}

impl Period {
    pub fn new() -> Period {
        Period {
            start: Local::now(),
            len: Duration::from_secs(0),
        }
    }

    pub fn finalize(&mut self) {
        self.len = Duration::from_secs(std::time::UNIX_EPOCH.elapsed().unwrap().as_secs() - self.start.timestamp() as u64);
    }

    pub fn date(&self) -> String {
        format!("{}", self.start.format("%a %b %e %T"))
    }

    pub fn elapsed(&self) -> Duration {
        if self.len.as_secs() == 0 {
            Duration::from_secs(std::time::UNIX_EPOCH.elapsed().unwrap().as_secs() - self.start.timestamp() as u64)
        } else {
            self.len
        }
    }
}

struct TimeTracker {
    start: DateTime<Local>,
    uptime: Option<Period>,
    uptimes: Vec<Period>,
    downtime: Option<Period>,
    downtimes: Vec<Period>,
}

impl TimeTracker {
    pub fn new() -> TimeTracker {
        let mut uptimes = Vec::new();
        uptimes.push(Period::new());

        TimeTracker {
            start: Local::now(),
            uptime: Some(Period::new()),
            uptimes: uptimes,
            downtime: None,
            downtimes: Vec::new(),
        }
    }

    pub fn down(&mut self) {
        if let Some(last) = self.uptimes.last_mut() {
            last.finalize();
        }

        self.uptime = None;
        self.downtime = Some(Period::new());
        self.downtimes.push(Period::new())
    }

    pub fn up(&mut self) {
        if let Some(last) = self.downtimes.last_mut() {
            last.finalize();
        }

        self.uptime = Some(Period::new());
        self.downtime = None;
        self.uptimes.push(Period::new());
    }

    pub fn is_down(&self) -> bool {
        self.downtime.is_some()
    }

    pub fn is_up(&self) -> bool {
        self.uptime.is_some()
    }

    pub fn downtime_str(&self) -> String {
        self.downtime.and_then(|inst| {
            Some(format_duration(inst.elapsed()))
        }).unwrap_or_else(|| "00:00:00".into())
    }

    pub fn uptime_str(&self) -> String {
        self.uptime.and_then(|inst| {
            Some(format_duration(inst.elapsed()))
        }).unwrap_or_else(|| "00:00:00".into())
    }

    pub fn total_downtime_str(&self) -> String {
        let total = self.downtimes.iter().fold(0, |sum, period| {
            sum + period.elapsed().as_secs()
        });

        format_duration(Duration::from_secs(total))
    }

    pub fn total_uptime_str(&self) -> String {
        let total = self.uptimes.iter().fold(0, |sum, period| {
            sum + period.elapsed().as_secs()
        });
        
        format_duration(Duration::from_secs(total))
    }

    pub fn downtimes(&self) -> &Vec<Period> {
        &self.downtimes
    }

    pub fn uptime_percentage(&self) -> f64 {
        let total_up = self.uptimes.iter().fold(0, |sum, period| {
            sum + period.elapsed().as_secs()
        }) as f64;

        let total = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs() - self.start.timestamp() as u64;
        total_up / total as f64
    }

    pub fn longest_uptime_str(&self) -> String {
        let max = self.uptimes.iter().max_by(|x, y| {
            x.elapsed().cmp(&y.elapsed())
        });

        if let Some(cur_time) = self.uptime {
            match max {
                Some(time) => {
                    if cur_time.elapsed() > time.elapsed() {
                        return self.uptime_str()
                    }
                }
                None => return self.uptime_str(),
            }
            
        }

        max.and_then(|inst| {
            Some(format_duration(inst.elapsed()))
        }).unwrap_or_else(|| "00:00:00".into())
    }

    pub fn longest_downtime_str(&self) -> String {
        let max = self.downtimes.iter().max_by(|x, y| {
            x.elapsed().cmp(&y.elapsed())
        });

        if let Some(cur_time) = self.downtime {
            match max {
                Some(time) => {
                    if cur_time.elapsed() > time.elapsed() {
                        return self.uptime_str()
                    }
                }
                None => return self.uptime_str(),
            }
            
        }

        max.and_then(|inst| {
            Some(format_duration(inst.elapsed()))
        }).unwrap_or_else(|| "00:00:00".into())
    }
}

struct PingResponse {
    pub dropped: bool,
    pub latency_ms: f64,
    pub hostname: String,
}

impl PingResponse {
    pub fn new(resp: &PingItem) -> PingResponse {
        PingResponse {
            dropped: resp.dropped == 1,
            latency_ms: resp.latency_ms,
            hostname: resp.hostname.clone(),
        }
    }
}

enum PingStatus {
    Responses(Vec<PingResponse>),
}

fn main() {
    let window = initscr();
    let ping = window.subwin(7, 38, 0, 2).expect("Could not make ping window.");
    let stats = window.subwin(9, 38, 0, 41).expect("Could not make stats window.");
    let mut down_list = window.subwin(window.get_max_y() - 10, 38, 8, 2).expect("Could not make downtime window.");
    
    window.nodelay(true);
    noecho();

    if has_colors() {
        start_color();
    }

    curs_set(0);

    for (i, color) in COLOR_TABLE.into_iter().enumerate() {
        init_pair(i as i16, *color, COLOR_BLACK);
    }
    
    let mut tracker = TimeTracker::new();

    let (sender, recver) = mpsc::channel();

    thread::spawn(move|| {
        loop {
            let mut ping = Ping::new();

            let res = ping.set_timeout(2.0)
                .and_then(|_| ping.add_host("8.8.8.8")
                    .and_then(|_| ping.add_host("4.2.2.2")
                        .and_then(|_| ping.add_host("208.67.222.222"))));

            if res.is_err() {
                continue;
            }
            
            let responses = match ping.send() {
                Ok(resp) => resp,
                _ => continue,
            };

            let mut resp = Vec::new();

            for res in responses {
                resp.push(PingResponse::new(&res));
            }

            let _ = sender.send(PingStatus::Responses(resp));

            thread::sleep(Duration::from_secs(1));
        }
    });

    let mut list_selection = 0;

    loop {
        match window.getch() {
            Some(Input::Character('q')) => break,
            Some(Input::Character('f')) => {
                beep();
            },
            Some(Input::Character('u')) => {
                if list_selection > 0 {
                    list_selection -= 1
                } else {
                    flash();
                }
            }
            Some(Input::Character('d')) => {
                if list_selection < tracker.downtimes().len() {
                    list_selection += 1
                } else {
                    flash();
                }
            }
            Some(Input::KeyResize) => {
                down_list = window
                    .subwin(window.get_max_y() - 10, 38, 8, 2)
                    .expect("Could not make downtime window.");
                
                window.mv(8, 0);
                window.clrtobot();
            }
            Some(key) => {
                window.mvaddstr(window.get_max_y() - 1, 0, &format!("{:?}", key));
            }
            _ => (),
        }

        if let Ok(status) = recver.try_recv() {
            match status {
                PingStatus::Responses(responses) => {
                    clear_err(&window);
                    ping.clear();
                    ping.draw_box(0, 0);

                    ping.attrset(A_BOLD);
                    ping.mvaddstr(0, 13, "Ping Status");
                    ping.attrset(A_NORMAL);

                    let mut dropped = 0;

                    for (host_num, resp) in responses.iter().enumerate() {
                        if resp.dropped {
                            dropped += 1;
                            print_host(&ping, false, resp, host_num);
                        }
                        else {
                            print_host(&ping, true, resp, host_num);
                        }
                    }

                    if dropped == 3 && tracker.is_up() {
                        tracker.down();
                    }
                    else if tracker.is_down() && dropped != 3 {
                        tracker.up();
                    }

                    ping.refresh();
                }
            }
        }

        print_stats(&stats, &tracker);
        print_downtimes(&down_list, &tracker, list_selection);
    }


    stats.delwin();
    down_list.delwin();
    ping.delwin();
    window.delwin();

    endwin();
}

fn print_stats(window: &Window, tracker: &TimeTracker) {
    window.draw_box(0, 0);
    let cols = window.get_max_x();

    window.attrset(A_BOLD);
    window.mvaddstr(0, (cols / 2) - 5, "Statistics");
    window.attrset(A_NORMAL);

    let percent_up = tracker.uptime_percentage() * 100.0;

    window.mvaddstr(1, 2, 
        &format!("Uptime        : {}", tracker.uptime_str()));

    window.mvaddstr(2, 2, 
        &format!("Max Uptime    : {}", tracker.longest_uptime_str()));

    window.mvaddstr(3, 2, 
        &format!("Total Uptime  : {}", tracker.total_uptime_str()));
    
    window.printw(" (");

    if percent_up <= 50.0 {
        window.attrset(COLOR_PAIR(4));
    }
    else if percent_up <= 80.0 {
        window.attrset(COLOR_PAIR(6));
    } 
    else {
        window.attrset(COLOR_PAIR(2));
    }

    window.printw(&format!("{:.2}", percent_up));
    window.attrset(COLOR_PAIR(7));
    window.printw("%) ");

    window.mvaddstr(5, 2, 
        &format!("Downtime      : {}", tracker.downtime_str()));

    window.mvaddstr(6, 2, 
        &format!("Max Downtime  : {}", tracker.longest_downtime_str()));

    window.mvaddstr(7, 2, 
        &format!("Total Downtime: {}", tracker.total_downtime_str()));
    refresh_window(window);
}

fn print_downtimes(window: &Window, tracker: &TimeTracker, select: usize) {
    window.draw_box(0, 0);
    let rows = window.get_max_y();
    let cols = window.get_max_x();
    let downtimes = tracker.downtimes();

    let total_down = &format!(": {}", downtimes.len());

    window.attrset(A_BOLD);
    window.mvaddstr(0, (cols / 2) - ((13 + total_down.len()) / 2) as i32, "Total Outages");
    window.attrset(A_NORMAL);
    window.printw(total_down);

    for (index, period) in downtimes.iter().rev().take(rows as usize - 2).enumerate() {
        window.mvaddstr(1 + index as i32, 1, "[");
        if select == downtimes.len() - index {
            window.attrset(A_BOLD);
            window.printw(&format!("{:>4}",
                downtimes.len() - index));
            window.attrset(A_NORMAL);
        } 
        else {
            window.printw(&format!("{:>4}",
                downtimes.len() - index));
        }
        window.printw(&format!("] {}: {}",
                period.date(),
                format_duration(period.elapsed())));
    }

    refresh_window(window);
}

fn clear_err(window: &Window) {
    let rows = window.get_max_y();
    let cols = window.get_max_x();

    window.mv(rows - 1, 0);
    window.hline(' ', cols);
}

fn print_host(window: &Window, passed: bool, resp: &PingResponse, host_num: usize) {
    if passed {
        let mut parts = resp.hostname.split('.');

        window.mvaddstr(host_num as i32 + 2, 1, "[");
        window.attrset(COLOR_PAIR(2));
        window.printw("PASS");
        window.attrset(COLOR_PAIR(7));
        window.printw(&format!("]: {:>3}.{:>3}.{:>3}.{:>3} (",
            parts.nth(0).unwrap(),
            parts.nth(0).unwrap(),
            parts.nth(0).unwrap(),
            parts.nth(0).unwrap()));

        if resp.latency_ms < 50.0 {
            window.attrset(COLOR_PAIR(2));
        }
        else if resp.latency_ms < 100.0 {
            window.attrset(COLOR_PAIR(6));
        }
        else {
            window.attrset(COLOR_PAIR(4));
        }

        window.printw(&format!("{:.2}", resp.latency_ms));
        window.attrset(COLOR_PAIR(7));
        window.printw(" ms)");
    }
    else {
        window.mvaddstr(host_num as i32 + 2, 1, "[");
        window.attrset(COLOR_PAIR(4));
        window.printw("FAIL");
        window.attrset(COLOR_PAIR(7));
        window.printw(&format!("]: {:>14}", resp.hostname));
    }
}

fn refresh_window(window: &Window) {
    napms(10);
    window.mv(window.get_max_y() - 1, window.get_max_x() - 1);
    window.refresh();
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
