//! Own the bundled private-race helper for the lifetime of one online session.
use std::io::{self, BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub enum Event {
    Hosted,
    Join(u16),
    Closed(String),
}

pub struct Tunnel {
    child: Child,
    events: Receiver<Event>,
    pub ready: bool,
}

impl Tunnel {
    pub fn start(host_port: Option<u16>) -> io::Result<Self> {
        let helper = std::env::current_exe()?
            .with_file_name(format!("scrapburn-tailcat{}", std::env::consts::EXE_SUFFIX));
        let mut command = Command::new(helper);
        match host_port {
            Some(port) => {
                command.arg("desktop-host").arg(port.to_string());
            }
            None => {
                command.arg("desktop-join");
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let output = child.stdout.take().expect("stdout was piped");
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines().map_while(Result::ok) {
                let event = if line == "READY" && host_port.is_some() {
                    Event::Hosted
                } else if let Some(port) = line
                    .strip_prefix("PORT ")
                    .and_then(|p| p.parse::<u16>().ok())
                    .filter(|p| *p > 0)
                    && host_port.is_none()
                {
                    Event::Join(port)
                } else if line == "CANCEL" {
                    Event::Closed(String::new())
                } else if let Some(reason) = line.strip_prefix("ERROR ") {
                    Event::Closed(reason.to_uppercase())
                } else {
                    Event::Closed("PRIVATE CONNECTION FAILED".into())
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            events,
            ready: false,
        })
    }

    pub fn poll(&mut self) -> Option<Event> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Event::Closed("PRIVATE CONNECTION CLOSED".into()))
            }
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.child.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
