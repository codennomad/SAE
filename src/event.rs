use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, KeyEvent, KeyEventKind};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval, Duration};

#[derive(Clone, Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    Network(crate::network::NetworkEvent),
    Resize(u16, u16),
}

pub struct EventHandler {
    sender: UnboundedSender<Event>,
    receiver: UnboundedReceiver<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        let event_sender = sender.clone();
        
        // Spawn the event handling task
        tokio::spawn(async move {
            let mut event_stream = crossterm::event::EventStream::new();
            let mut tick_interval = interval(tick_rate);
            
            loop {
                let tick_delay = tick_interval.tick();
                let crossterm_event = event_stream.next().fuse();
                
                tokio::select! {
                    _ = tick_delay => {
                        if event_sender.send(Event::Tick).is_err() {
                            break;
                        }
                    }
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(evt)) => {
                                match evt {
                                    CrosstermEvent::Key(key) => {
                                        // Only process KeyEventKind::Press to avoid duplicate events
                                        if key.kind == KeyEventKind::Press {
                                            if event_sender.send(Event::Key(key)).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    CrosstermEvent::Resize(w, h) => {
                                        if event_sender.send(Event::Resize(w, h)).is_err() {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Some(Err(_)) => {
                                // Error reading event, continue
                            }
                            None => {
                                // Stream ended
                                break;
                            }
                        }
                    }
                }
            }
        });
        
        Self { sender, receiver }
    }
    
    pub async fn next(&mut self) -> Result<Event> {
        self.receiver.recv().await
            .ok_or_else(|| color_eyre::eyre::eyre!("Event channel closed"))
    }
    
    pub fn sender(&self) -> UnboundedSender<Event> {
        self.sender.clone()
    }
}