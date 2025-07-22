# TUI Radar Simulation

![demo](demo.gif)

This is a simple "radar sim" I made using Ratatui, mostly to try out creating an event/render loop without tokio and just using threads.

Using `mpsc` (multi-producer single consumer) channels to pass messages from background threads back to my main loop to be processed without blocking. It's possible to have tick and render timing in the main loop as well, but it does cause fps drops when you poll for input.

The threadpool isn't being used at all and was just something left over.

## How it works

I spawn a few threads that send messages to the main thread:

#### Messages look like this:
```rust
pub enum Message {
    Quit,
    Tick,
    Render,
    KeyPress(KeyCode),
}
```

#### Tick thread sends timing messages:
```rust
let tick_tx = self.msg_tx.clone();
thread::spawn(move || {
    loop {
        thread::sleep(tick_duration);
        if tick_tx.send(Message::Tick).is_err() {
            break;
        }
    }
});
```

#### Main loop processes everything:
```rust
loop {
    if let Ok(msg) = self.msg_rx.recv_timeout(Duration::from_millis(100)) {
        match self.update(&msg)? {
            UpdateCommand::None => {}
            UpdateCommand::Quit => break,
        }
    }
}
```

The radar sweeps around and detects different types of objects (aircraft, ships, etc) that move around and fade out over time or are removed if out of range


## Why threads instead of async?
I just wanted to see how it felt compared to tokio. For a simple TUI like this, threads are actually pretty nice, no async complexity and the performance is fine.

## Learn more about radar
- [How Radar Works](https://www.youtube.com/watch?v=c8OWHnHjIpA)
- [How Radars Tell Targets Apart](https://www.youtube.com/watch?v=MmpPfQ8WoWk)