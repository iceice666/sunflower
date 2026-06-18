package sig_test

import (
	"context"
	"encoding/json"
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

type nsigFixture struct {
	FuncName string `json:"func_name"`
	FuncBody string `json:"func_body"`
	Cases    []struct {
		In  string `json:"in"`
		Out string `json:"out"`
	} `json:"cases"`
}

func TestDecodeNFromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/nsig_fixture.json")
	if err != nil {
		t.Fatal(err)
	}
	var fix nsigFixture
	if err := json.Unmarshal(raw, &fix); err != nil {
		t.Fatal(err)
	}

	cache := sig.NewCache(nil)
	if err := cache.LoadNsigForTest(fix.FuncName, fix.FuncBody); err != nil {
		t.Fatalf("LoadNsigForTest: %v", err)
	}

	for _, tc := range fix.Cases {
		got, err := cache.DecodeNRaw(context.Background(), tc.In)
		if err != nil {
			t.Errorf("DecodeNRaw(%q) error: %v", tc.In, err)
			continue
		}
		if got != tc.Out {
			t.Errorf("DecodeNRaw(%q) = %q, want %q", tc.In, got, tc.Out)
		}
	}
}
