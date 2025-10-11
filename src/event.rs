use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, KeyEvent, KeyEventKind};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval, Duration};

#[derive(Clone, Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    Network(crate::network_secure::NetworkEvent),
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
        
        tokio::spawn(async move {
            let mut event_stream = crossterm::event::EventStream::new();
            let mut tick_interval = interval(tick_rate);
            
            loop {
                let tick_delay = tick_interval.tick();
                let crossterm_event = event_stream.next().fuse();
                
                tokio::select! {
                    _ = tick_delay => {
                        event_sender.send(Event::Tick).unwrap();
                    }
                    Some(Ok(evt)) = crossterm_event => {
                        match evt {
                            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                                event_sender.send(Event::Key(key)).unwrap();
                            }
                            CrosstermEvent::Resize(w, h) => {
                                event_sender.send(Event::Resize(w, h)).unwrap();
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
        
        Self { sender, receiver }
    }
    
    pub async fn next(&mut self) -> Result<Event> {
        self.receiver.recv().await
            .ok_or_else(|| color_eyre::eyre::eyre!("Canal de eventos fechado"))
    }
    
    pub fn sender(&self) -> UnboundedSender<Event> {
        self.sender.clone()
    }
}
