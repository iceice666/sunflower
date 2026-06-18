package ws

import (
	"net/http"
	"time"

	"github.com/gorilla/websocket"
)

const (
	writeWait      = 10 * time.Second
	pongWait       = 60 * time.Second
	pingPeriod     = (pongWait * 9) / 10 // ping before pong deadline
	maxMessageSize = 8 * 1024
	sendBuffer     = 32
)

var upgrader = websocket.Upgrader{
	Subprotocols: []string{Subprotocol},
	// Single-user, LAN-oriented service: accept any origin. Tighten for a
	// hardened deployment.
	CheckOrigin: func(_ *http.Request) bool { return true },
}

// Conn is a single now-playing WebSocket connection bound to a device.
type Conn struct {
	hub      *Hub
	ws       *websocket.Conn
	deviceID string
	send     chan []byte
}

// Serve upgrades the HTTP request to a WebSocket, registers the connection, and
// runs its read and write pumps until the socket closes. deviceID identifies the
// authenticated device this connection belongs to.
func Serve(h *Hub, deviceID string, w http.ResponseWriter, r *http.Request) error {
	c, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		return err
	}
	conn := &Conn{hub: h, ws: c, deviceID: deviceID, send: make(chan []byte, sendBuffer)}
	h.register(conn)

	// Seed the new observer with the current snapshot so it renders immediately
	// instead of waiting for the next tick.
	for _, np := range h.Snapshot() {
		if np.DeviceID == deviceID {
			continue
		}
		if payload, err := encode(clientFromNowPlaying(np)); err == nil {
			conn.trySend(payload)
		}
	}

	go conn.writePump()
	conn.readPump() // blocks until the connection closes
	return nil
}

// trySend enqueues a frame without blocking. Returns false (and closes the
// connection's send channel via the write pump) when the buffer is full — a slow
// observer is dropped rather than stalling the hub.
func (c *Conn) trySend(b []byte) bool {
	select {
	case c.send <- b:
		return true
	default:
		return false
	}
}

func (c *Conn) readPump() {
	defer func() {
		c.hub.unregister(c)
		_ = c.ws.Close()
		close(c.send)
	}()
	c.ws.SetReadLimit(maxMessageSize)
	_ = c.ws.SetReadDeadline(time.Now().Add(pongWait))
	c.ws.SetPongHandler(func(string) error {
		return c.ws.SetReadDeadline(time.Now().Add(pongWait))
	})
	for {
		_, raw, err := c.ws.ReadMessage()
		if err != nil {
			return
		}
		m, err := decodeClient(raw)
		if err != nil {
			continue // ignore malformed frames
		}
		c.hub.onClientMessage(c, m)
	}
}

func (c *Conn) writePump() {
	ticker := time.NewTicker(pingPeriod)
	defer ticker.Stop()
	for {
		select {
		case msg, ok := <-c.send:
			_ = c.ws.SetWriteDeadline(time.Now().Add(writeWait))
			if !ok {
				_ = c.ws.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}
			if err := c.ws.WriteMessage(websocket.TextMessage, msg); err != nil {
				return
			}
		case <-ticker.C:
			_ = c.ws.SetWriteDeadline(time.Now().Add(writeWait))
			if err := c.ws.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}
}

// clientFromNowPlaying re-expresses a stored NowPlaying as a tick frame for a
// late-joining observer's initial render.
func clientFromNowPlaying(np NowPlaying) ClientMessage {
	return ClientMessage{
		Type:       KindTick,
		QueueID:    np.QueueID,
		MediaID:    np.MediaID,
		Title:      np.Title,
		Artist:     np.Artist,
		PositionMs: np.PositionMs,
		DurationMs: np.DurationMs,
		IsPlaying:  np.IsPlaying,
	}
}
