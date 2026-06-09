package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/iceice666/sunflower/server/internal/api"
	"github.com/rs/zerolog"
)

// TestHealthz verifies the /healthz endpoint without a database.
func TestHealthz(t *testing.T) {
	handler := api.NewRouter(api.Deps{Log: zerolog.Nop()})

	req := httptest.NewRequest(http.MethodGet, "/healthz", nil)
	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, req)

	res := rec.Result()
	defer res.Body.Close()

	if res.StatusCode != http.StatusOK {
		t.Fatalf("expected 200, got %d", res.StatusCode)
	}

	ct := res.Header.Get("Content-Type")
	if ct == "" {
		t.Fatal("missing Content-Type header")
	}

	var body map[string]string
	if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
		t.Fatalf("decode body: %v", err)
	}
	if body["status"] != "ok" {
		t.Fatalf(`expected {"status":"ok"}, got %v`, body)
	}
}
