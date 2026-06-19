package streamproxy

import (
	"testing"
	"time"
)

func TestSignVerifyRoundTrip(t *testing.T) {
	s := NewSigner([]byte("test-key-0123456789"), time.Minute)
	target := "https://r1---sn-abc.googlevideo.com/videoplayback?expire=123"

	tok := s.Sign(target)
	got, err := s.Verify(tok)
	if err != nil {
		t.Fatalf("Verify: %v", err)
	}
	if got != target {
		t.Fatalf("round-trip url = %q, want %q", got, target)
	}
}

func TestVerifyRejectsTamperedSignature(t *testing.T) {
	s := NewSigner([]byte("key"), time.Minute)
	tok := s.Sign("https://x.googlevideo.com/a")

	bad := tok[:len(tok)-1] + string(flip(tok[len(tok)-1]))
	if _, err := s.Verify(bad); err == nil {
		t.Fatal("expected error for tampered signature, got nil")
	}
}

func TestVerifyRejectsWrongKey(t *testing.T) {
	tok := NewSigner([]byte("key-a"), time.Minute).Sign("https://x.googlevideo.com/a")
	if _, err := NewSigner([]byte("key-b"), time.Minute).Verify(tok); err == nil {
		t.Fatal("expected error verifying with a different key")
	}
}

func TestVerifyRejectsExpired(t *testing.T) {
	now := time.Now()
	s := NewSigner([]byte("key"), time.Minute)
	s.now = func() time.Time { return now }
	tok := s.Sign("https://x.googlevideo.com/a")

	// Advance past the TTL.
	s.now = func() time.Time { return now.Add(2 * time.Minute) }
	if _, err := s.Verify(tok); err != ErrInvalidToken {
		t.Fatalf("expected ErrInvalidToken for expired token, got %v", err)
	}
}

func TestSignUntilAlignsExpiry(t *testing.T) {
	now := time.Now()
	s := NewSigner([]byte("key"), time.Minute) // default ttl is 1m
	s.now = func() time.Time { return now }

	// A token signed to live well past the default ttl stays valid past it.
	tok := s.SignUntil("https://x.googlevideo.com/a", now.Add(time.Hour))
	s.now = func() time.Time { return now.Add(30 * time.Minute) }
	if _, err := s.Verify(tok); err != nil {
		t.Fatalf("token should outlive default ttl: %v", err)
	}

	// Past its own expiry it is rejected.
	s.now = func() time.Time { return now.Add(2 * time.Hour) }
	if _, err := s.Verify(tok); err != ErrInvalidToken {
		t.Fatalf("expected ErrInvalidToken past expiry, got %v", err)
	}
}

func TestVerifyRejectsMalformed(t *testing.T) {
	s := NewSigner([]byte("key"), time.Minute)
	for _, tok := range []string{"", "nodot", ".", "a.", ".b", "not.base64!!"} {
		if _, err := s.Verify(tok); err == nil {
			t.Fatalf("expected error for malformed token %q", tok)
		}
	}
}

func flip(b byte) byte {
	if b == 'A' {
		return 'B'
	}
	return 'A'
}
