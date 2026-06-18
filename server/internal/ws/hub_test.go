package ws

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"github.com/rs/zerolog"
)

// wsURL converts an http(s) test-server URL to a ws(s) URL.
func wsURL(s string) string {
	return "ws" + strings.TrimPrefix(s, "http")
}

// TestHubBroadcastAndCommand connects a "player" and an "observer", sends a tick
// from the player, asserts the observer receives it, then issues a pause command
// to the player and asserts delivery — the core M8 loop.
func TestHubBroadcastAndCommand(t *testing.T) {
	hub := NewHub(zerolog.Nop())

	// Test server: the first connection is the player, the rest are observers.
	// The device id is taken from the ?device query param for the test.
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = Serve(hub, r.URL.Query().Get("device"), w, r)
	}))
	defer srv.Close()

	dialer := websocket.Dialer{Subprotocols: []string{Subprotocol}}

	player, _, err := dialer.Dial(wsURL(srv.URL)+"?device=playerDev", nil)
	if err != nil {
		t.Fatalf("dial player: %v", err)
	}
	defer player.Close()

	observer, _, err := dialer.Dial(wsURL(srv.URL)+"?device=observerDev", nil)
	if err != nil {
		t.Fatalf("dial observer: %v", err)
	}
	defer observer.Close()

	// Give both connections a moment to register.
	time.Sleep(100 * time.Millisecond)

	// Player sends a tick.
	tick := ClientMessage{Type: KindTick, MediaID: "yt:abc", PositionMs: 5000, IsPlaying: true}
	raw, _ := encode(tick)
	if err := player.WriteMessage(websocket.TextMessage, raw); err != nil {
		t.Fatalf("player write: %v", err)
	}

	// Observer should receive the broadcast.
	_ = observer.SetReadDeadline(time.Now().Add(2 * time.Second))
	_, got, err := observer.ReadMessage()
	if err != nil {
		t.Fatalf("observer read: %v", err)
	}
	m, err := decodeClient(got)
	if err != nil || m.MediaID != "yt:abc" || m.PositionMs != 5000 {
		t.Fatalf("observer got wrong frame: %+v err=%v", m, err)
	}

	// Snapshot reflects the player's state.
	snap := hub.Snapshot()
	found := false
	for _, np := range snap {
		if np.DeviceID == "playerDev" && np.MediaID == "yt:abc" {
			found = true
		}
	}
	if !found {
		t.Fatalf("snapshot missing player state: %+v", snap)
	}

	// Controller issues a pause command to the player.
	if n := hub.SendCommand("playerDev", CmdPause); n != 1 {
		t.Fatalf("SendCommand delivered to %d conns, want 1", n)
	}
	_ = player.SetReadDeadline(time.Now().Add(2 * time.Second))
	_, cmdRaw, err := player.ReadMessage()
	if err != nil {
		t.Fatalf("player read command: %v", err)
	}
	if !strings.Contains(string(cmdRaw), `"command":"pause"`) {
		t.Fatalf("player got unexpected command frame: %s", cmdRaw)
	}
}

// TestSubprotocolNegotiated asserts the server selects the now-playing
// subprotocol during the handshake.
func TestSubprotocolNegotiated(t *testing.T) {
	hub := NewHub(zerolog.Nop())
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = Serve(hub, "dev", w, r)
	}))
	defer srv.Close()

	dialer := websocket.Dialer{Subprotocols: []string{Subprotocol}}
	conn, resp, err := dialer.Dial(wsURL(srv.URL), nil)
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	defer conn.Close()
	if resp.Header.Get("Sec-WebSocket-Protocol") != Subprotocol {
		t.Fatalf("subprotocol not negotiated: %q", resp.Header.Get("Sec-WebSocket-Protocol"))
	}
}
