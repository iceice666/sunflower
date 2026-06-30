package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"strconv"
	"strings"

	"golang.org/x/crypto/argon2"
)

var ErrWeakPassword = errors.New("weak_password")

// HashPassword stores owner/admin passwords in PHC argon2id format.
func HashPassword(password string) (string, error) {
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return "", err
	}
	hash := argon2.IDKey([]byte(password), salt, argonTime, argonMemory, argonThreads, argonKeyLen)
	enc := base64.RawStdEncoding
	return fmt.Sprintf("$argon2id$v=19$m=%d,t=%d,p=%d$%s$%s",
		argonMemory, argonTime, argonThreads, enc.EncodeToString(salt), enc.EncodeToString(hash)), nil
}

// VerifyPassword checks a PHC argon2id password hash.
func VerifyPassword(phc, password string) (bool, error) {
	parts := strings.Split(phc, "$")
	if len(parts) != 6 || parts[1] != "argon2id" {
		return false, fmt.Errorf("unsupported password hash")
	}
	params := map[string]int{}
	for _, kv := range strings.Split(parts[3], ",") {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return false, fmt.Errorf("invalid password params")
		}
		n, err := strconv.Atoi(v)
		if err != nil {
			return false, err
		}
		params[k] = n
	}
	enc := base64.RawStdEncoding
	salt, err := enc.DecodeString(parts[4])
	if err != nil {
		return false, err
	}
	want, err := enc.DecodeString(parts[5])
	if err != nil {
		return false, err
	}
	hash := argon2.IDKey([]byte(password), salt,
		uint32(params["t"]), uint32(params["m"]), uint8(params["p"]), uint32(len(want)))
	return subtle.ConstantTimeCompare(hash, want) == 1, nil
}

// ValidateOwnerPassword applies the M9 minimum policy.
func ValidateOwnerPassword(password, setupToken string) error {
	p := strings.TrimSpace(password)
	if p == "" || len(p) < 12 {
		return ErrWeakPassword
	}
	if setupToken != "" && subtle.ConstantTimeCompare([]byte(p), []byte(strings.TrimSpace(setupToken))) == 1 {
		return ErrWeakPassword
	}
	return nil
}
