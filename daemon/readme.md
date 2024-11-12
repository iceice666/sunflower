

# sunflower-daemon

Support following method to transfer message:  
- TCP: `localhost:8888`
- Windows Named Pipe:  `\\.\pipe\sunflower-daemon`
- Unix Socket: `/tmp/sunflower-daemon.sock`

The message scheme is defined with ProtoBuf.
Can be found at `../proto/src/.proto`

## Road Map

- [ ] macOS multimedia keys support
- [ ] Windows Media Control