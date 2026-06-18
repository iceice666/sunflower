package ws

import "testing"

func TestProtocolRoundTrip(t *testing.T) {
	in := ClientMessage{
		Type:       KindTick,
		QueueID:    "q1",
		MediaID:    "yt:abc",
		Title:      "Song",
		Artist:     "Artist",
		PositionMs: 12345,
		DurationMs: 200000,
		IsPlaying:  true,
	}
	raw, err := encode(in)
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	out, err := decodeClient(raw)
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out != in {
		t.Fatalf("round-trip mismatch:\n got %+v\nwant %+v", out, in)
	}
}

func TestDecodeIgnoresUnknownFields(t *testing.T) {
	raw := []byte(`{"type":"tick","media_id":"x","extra_field":42,"is_playing":true}`)
	m, err := decodeClient(raw)
	if err != nil {
		t.Fatalf("decode with unknown field: %v", err)
	}
	if m.Type != KindTick || m.MediaID != "x" || !m.IsPlaying {
		t.Fatalf("unexpected decode: %+v", m)
	}
}

func TestServerCommandEncode(t *testing.T) {
	raw, err := encode(ServerMessage{Type: KindCommand, Command: CmdPause})
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	if string(raw) != `{"type":"command","command":"pause"}` {
		t.Fatalf("unexpected command JSON: %s", raw)
	}
}
