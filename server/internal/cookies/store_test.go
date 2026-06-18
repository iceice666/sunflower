package cookies_test

import (
	"testing"

	"github.com/iceice666/sunflower/server/internal/cookies"
)

func TestStoreRoundTrip(t *testing.T) {
	var key [32]byte
	copy(key[:], "test-key-32-bytes-padded-to-fit!!")

	plain := []byte("cookie_data=abc123; domain=youtube.com")

	ct, nonce, err := cookies.Encrypt(key, plain)
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	got, err := cookies.Decrypt(key, ct, nonce)
	if err != nil {
		t.Fatalf("Decrypt: %v", err)
	}
	if string(got) != string(plain) {
		t.Errorf("got %q, want %q", got, plain)
	}
}

func TestDecryptTampered(t *testing.T) {
	var key [32]byte
	copy(key[:], "test-key-32-bytes-padded-to-fit!!")

	ct, nonce, _ := cookies.Encrypt(key, []byte("data"))
	ct[0] ^= 0xFF // tamper

	_, err := cookies.Decrypt(key, ct, nonce)
	if err != cookies.ErrDecryptFailed {
		t.Errorf("expected ErrDecryptFailed, got %v", err)
	}
}
