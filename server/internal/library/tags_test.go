package library

import (
	"bytes"
	"os"
	"path/filepath"
	"strconv"
	"testing"
)

func TestMediaIDStability(t *testing.T) {
	id1 := localMediaID("/music/foo.mp3")
	id2 := localMediaID("/music/foo.mp3")
	if id1 != id2 {
		t.Fatalf("same path produced different IDs: %q vs %q", id1, id2)
	}
	if id3 := localMediaID("/music/bar.mp3"); id1 == id3 {
		t.Fatal("different paths produced the same ID")
	}
	// "local:" (6 chars) + 16 hex chars
	if len(id1) != 22 {
		t.Fatalf("unexpected ID length %d: %q", len(id1), id1)
	}
	if id1[:6] != "local:" {
		t.Fatalf("ID does not start with 'local:': %q", id1)
	}
}

func TestExtractTagsBasic(t *testing.T) {
	data := makeMinimalMP3("Test Song", "Test Artist", "Test Album", 1, 2023)
	path := filepath.Join(t.TempDir(), "test.mp3")
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}

	meta, err := ExtractTags(path)
	if err != nil {
		t.Fatalf("ExtractTags: %v", err)
	}
	if meta.Title != "Test Song" {
		t.Errorf("title: got %q, want %q", meta.Title, "Test Song")
	}
	if meta.Artist != "Test Artist" {
		t.Errorf("artist: got %q, want %q", meta.Artist, "Test Artist")
	}
	if meta.Album != "Test Album" {
		t.Errorf("album: got %q, want %q", meta.Album, "Test Album")
	}
	if meta.Year != 2023 {
		t.Errorf("year: got %d, want %d", meta.Year, 2023)
	}

	absPath, _ := filepath.Abs(path)
	if meta.MediaID != localMediaID(absPath) {
		t.Errorf("media_id mismatch: got %q, want %q", meta.MediaID, localMediaID(absPath))
	}
}

func TestExtractTagsIDStabilityAcrossCalls(t *testing.T) {
	data := makeMinimalMP3("Stable Song", "Stable Artist", "Stable Album", 2, 2020)
	path := filepath.Join(t.TempDir(), "stable.mp3")
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}

	m1, err := ExtractTags(path)
	if err != nil {
		t.Fatal(err)
	}
	m2, err := ExtractTags(path)
	if err != nil {
		t.Fatal(err)
	}
	if m1.MediaID != m2.MediaID {
		t.Fatalf("media_id changed between calls: %q vs %q", m1.MediaID, m2.MediaID)
	}
}

// makeMinimalMP3 builds a minimal ID3v2.3-tagged byte slice that dhowden/tag
// can parse. No MPEG audio frames are included.
func makeMinimalMP3(title, artist, album string, trackNum, year int) []byte {
	var frames bytes.Buffer

	writeFrame := func(id, text string) {
		// Encoding byte 0 = ISO-8859-1 (only encoding defined in ID3v2.3)
		data := append([]byte{0}, []byte(text)...)
		size := len(data)
		frames.WriteString(id)
		frames.Write([]byte{byte(size >> 24), byte(size >> 16), byte(size >> 8), byte(size)})
		frames.Write([]byte{0, 0}) // frame flags
		frames.Write(data)
	}

	if title != "" {
		writeFrame("TIT2", title)
	}
	if artist != "" {
		writeFrame("TPE1", artist)
	}
	if album != "" {
		writeFrame("TALB", album)
	}
	if trackNum > 0 {
		writeFrame("TRCK", strconv.Itoa(trackNum))
	}
	if year > 0 {
		// TYER is the year frame in ID3v2.3; TDRC is ID3v2.4-only
		writeFrame("TYER", strconv.Itoa(year))
	}

	body := frames.Bytes()
	tagSize := len(body)

	var out bytes.Buffer
	out.WriteString("ID3")
	out.Write([]byte{3, 0, 0}) // ID3v2.3, minor 0, no flags
	// Syncsafe 28-bit tag size: each byte uses only 7 bits
	out.Write([]byte{
		byte((tagSize >> 21) & 0x7F),
		byte((tagSize >> 14) & 0x7F),
		byte((tagSize >> 7) & 0x7F),
		byte(tagSize & 0x7F),
	})
	out.Write(body)
	return out.Bytes()
}
