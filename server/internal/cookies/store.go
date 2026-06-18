// Package cookies provides secretbox-encrypted cookie storage backed by Postgres.
package cookies

import (
	"context"
	"crypto/rand"
	"errors"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"golang.org/x/crypto/nacl/secretbox"
)

var (
	ErrDecryptFailed = errors.New("cookies: decrypt failed")
	ErrNotFound      = errors.New("cookies: not found")
)

// Encrypt encrypts plain with secretbox and returns (ciphertext, nonce, error).
// The nonce and ciphertext are stored separately in the DB.
func Encrypt(key [32]byte, plain []byte) (ciphertext []byte, nonce [24]byte, err error) {
	if _, err = rand.Read(nonce[:]); err != nil {
		return nil, nonce, err
	}
	ct := secretbox.Seal(nil, plain, &nonce, &key)
	return ct, nonce, nil
}

// Decrypt decrypts ciphertext using key and nonce.
func Decrypt(key [32]byte, ciphertext []byte, nonce [24]byte) ([]byte, error) {
	plain, ok := secretbox.Open(nil, ciphertext, &nonce, &key)
	if !ok {
		return nil, ErrDecryptFailed
	}
	return plain, nil
}

// Store encrypts raw and upserts it into the encrypted_cookies table.
func Store(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string, raw []byte) error {
	ct, nonce, err := Encrypt(key, raw)
	if err != nil {
		return err
	}
	_, err = db.Exec(ctx, `
		INSERT INTO encrypted_cookies (user_id, provider, ciphertext, nonce, refreshed_at)
		VALUES ($1, $2, $3, $4, now())
		ON CONFLICT (user_id, provider) DO UPDATE
		SET ciphertext = EXCLUDED.ciphertext,
		    nonce      = EXCLUDED.nonce,
		    refreshed_at = now()
	`, userID, provider, ct, nonce[:])
	return err
}

// Load reads and decrypts the cookie for the given user/provider.
func Load(ctx context.Context, db *pgxpool.Pool, key [32]byte, userID uuid.UUID, provider string) ([]byte, error) {
	var ct []byte
	var nonceBytes []byte
	err := db.QueryRow(ctx,
		`SELECT ciphertext, nonce FROM encrypted_cookies WHERE user_id=$1 AND provider=$2`,
		userID, provider,
	).Scan(&ct, &nonceBytes)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	var nonce [24]byte
	copy(nonce[:], nonceBytes)
	return Decrypt(key, ct, nonce)
}
