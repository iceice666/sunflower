// Package ws implements the M8 now-playing WebSocket: a hub that relays the
// playing client's position ticks / transitions to observer clients (a second
// device or the admin dashboard) and forwards control commands back.
//
// Subprotocol: "sunflower.now-playing.v1".
package ws

import "encoding/json"

// Subprotocol is the negotiated WebSocket subprotocol for now-playing.
const Subprotocol = "sunflower.now-playing.v1"

// Message kinds (client → server).
const (
	KindTick       = "tick"       // ~1 Hz position update during playback
	KindTransition = "transition" // track changed
	KindState      = "state"      // play/pause/shuffle/repeat changed
)

// Message kind (server → client).
const (
	KindCommand = "command"
)

// Commands a controller can issue (server → playing client).
const (
	CmdPause    = "pause"
	CmdPlay     = "play"
	CmdSkipNext = "skip_next"
	CmdSkipPrev = "skip_prev"
)

// ClientMessage is a tick / transition / state frame sent by the playing client.
// Optional fields are omitted when zero so a paused state isn't a noisy full
// frame.
type ClientMessage struct {
	Type       string `json:"type"`
	QueueID    string `json:"queue_id,omitempty"`
	MediaID    string `json:"media_id,omitempty"`
	Title      string `json:"title,omitempty"`
	Artist     string `json:"artist,omitempty"`
	PositionMs int    `json:"position_ms,omitempty"`
	DurationMs int    `json:"duration_ms,omitempty"`
	IsPlaying  bool   `json:"is_playing"`
	Shuffle    bool   `json:"shuffle,omitempty"`
	Repeat     string `json:"repeat,omitempty"`
}

// ServerMessage is a command frame sent to the playing client.
type ServerMessage struct {
	Type    string `json:"type"`
	Command string `json:"command"`
}

// NowPlaying is the last-known state of a device, surfaced via /admin and
// broadcast to observers.
type NowPlaying struct {
	DeviceID   string `json:"device_id"`
	QueueID    string `json:"queue_id,omitempty"`
	MediaID    string `json:"media_id,omitempty"`
	Title      string `json:"title,omitempty"`
	Artist     string `json:"artist,omitempty"`
	PositionMs int    `json:"position_ms"`
	DurationMs int    `json:"duration_ms,omitempty"`
	IsPlaying  bool   `json:"is_playing"`
	UpdatedAt  string `json:"updated_at"` // RFC3339
}

// decodeClient parses a raw client frame. Unknown/extra fields are ignored.
func decodeClient(raw []byte) (ClientMessage, error) {
	var m ClientMessage
	err := json.Unmarshal(raw, &m)
	return m, err
}

// encode marshals any protocol message to JSON bytes.
func encode(v any) ([]byte, error) { return json.Marshal(v) }
