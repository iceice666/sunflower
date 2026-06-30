package auth

import (
	"sync"
	"time"
)

// RateLimiter is a tiny in-memory fixed-window limiter for security-sensitive
// setup/login/pairing endpoints. It is per-process and intentionally simple.
type RateLimiter struct {
	mu      sync.Mutex
	limit   int
	window  time.Duration
	entries map[string]rateEntry
}

type rateEntry struct {
	start time.Time
	count int
}

func NewRateLimiter(limit int, window time.Duration) *RateLimiter {
	return &RateLimiter{limit: limit, window: window, entries: make(map[string]rateEntry)}
}

func (l *RateLimiter) Allow(key string) bool {
	if l == nil {
		return true
	}
	now := time.Now()
	l.mu.Lock()
	defer l.mu.Unlock()
	e := l.entries[key]
	if e.start.IsZero() || now.Sub(e.start) > l.window {
		l.entries[key] = rateEntry{start: now, count: 1}
		return true
	}
	if e.count >= l.limit {
		return false
	}
	e.count++
	l.entries[key] = e
	return true
}

func (l *RateLimiter) Reset(key string) {
	if l == nil {
		return
	}
	l.mu.Lock()
	delete(l.entries, key)
	l.mu.Unlock()
}
