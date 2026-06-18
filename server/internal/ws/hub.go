package ws

import (
	"sync"
	"time"

	"github.com/rs/zerolog"
)

// Hub is the central registry of live now-playing connections. Each connection
// belongs to a device. The hub keeps the latest NowPlaying per device (for
// /admin and late-joining observers), broadcasts incoming ticks to every other
// connection, and routes commands to a specific device's connections.
//
// Safe for concurrent use.
type Hub struct {
	log zerolog.Logger

	mu    sync.RWMutex
	conns map[*Conn]struct{}
	// latest now-playing per device id.
	state map[string]NowPlaying
}

// NewHub builds an empty hub.
func NewHub(log zerolog.Logger) *Hub {
	return &Hub{
		log:   log,
		conns: make(map[*Conn]struct{}),
		state: make(map[string]NowPlaying),
	}
}

// register adds a connection.
func (h *Hub) register(c *Conn) {
	h.mu.Lock()
	h.conns[c] = struct{}{}
	h.mu.Unlock()
}

// unregister removes a connection.
func (h *Hub) unregister(c *Conn) {
	h.mu.Lock()
	delete(h.conns, c)
	h.mu.Unlock()
}

// onClientMessage updates per-device state and fans the frame out to observers
// (every connection except the sender). Called by Conn's read loop.
func (h *Hub) onClientMessage(sender *Conn, m ClientMessage) {
	np := NowPlaying{
		DeviceID:   sender.deviceID,
		QueueID:    m.QueueID,
		MediaID:    m.MediaID,
		Title:      m.Title,
		Artist:     m.Artist,
		PositionMs: m.PositionMs,
		DurationMs: m.DurationMs,
		IsPlaying:  m.IsPlaying,
		UpdatedAt:  time.Now().UTC().Format(time.RFC3339),
	}
	h.mu.Lock()
	h.state[sender.deviceID] = np
	targets := make([]*Conn, 0, len(h.conns))
	for c := range h.conns {
		if c != sender {
			targets = append(targets, c)
		}
	}
	h.mu.Unlock()

	payload, err := encode(m)
	if err != nil {
		return
	}
	for _, c := range targets {
		c.trySend(payload)
	}
}

// SendCommand routes a command to every connection of the target device.
// Returns the number of connections the command was delivered to.
func (h *Hub) SendCommand(deviceID, command string) int {
	msg, err := encode(ServerMessage{Type: KindCommand, Command: command})
	if err != nil {
		return 0
	}
	h.mu.RLock()
	targets := make([]*Conn, 0)
	for c := range h.conns {
		if c.deviceID == deviceID {
			targets = append(targets, c)
		}
	}
	h.mu.RUnlock()

	n := 0
	for _, c := range targets {
		if c.trySend(msg) {
			n++
		}
	}
	return n
}

// Snapshot returns the latest now-playing state for every known device — the
// /admin dashboard payload.
func (h *Hub) Snapshot() []NowPlaying {
	h.mu.RLock()
	defer h.mu.RUnlock()
	out := make([]NowPlaying, 0, len(h.state))
	for _, np := range h.state {
		out = append(out, np)
	}
	return out
}
