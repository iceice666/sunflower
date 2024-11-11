
# sunflower-rs [WIP]

A music player

## Daemon

Support following method to transfer message:  
- TCP: `localhost:8888`  
- Windows Named Pipe:  `\\.\pipe\sunflower-daemon`  
- Unix Socket: `/tmp/sunflower-daemon.sock`

The Message scheme is defined with ProtoBuf.
Can be found at `proto/src/.proto`

## Road Map

- [ ] CLI
- [ ] GUI
- [ ] macOS multimedia keys support
- [ ] Windows Media Control