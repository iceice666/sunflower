package sig_test

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

type sigFixture struct {
	Ops []struct {
		Kind string `json:"kind"`
		Arg  int    `json:"arg"`
	} `json:"ops"`
	Cases []struct {
		In  string `json:"in"`
		Out string `json:"out"`
	} `json:"cases"`
}

func TestApply_FromFixture(t *testing.T) {
	raw, err := os.ReadFile("testdata/sig_fixture.json")
	if err != nil {
		t.Fatal(err)
	}
	var fix sigFixture
	if err := json.Unmarshal(raw, &fix); err != nil {
		t.Fatal(err)
	}

	ops := make([]sig.Op, len(fix.Ops))
	for i, o := range fix.Ops {
		ops[i] = sig.Op{Kind: sig.OpKindFromString(o.Kind), Arg: o.Arg}
	}

	for _, tc := range fix.Cases {
		got := sig.Apply(ops, tc.In)
		if got != tc.Out {
			t.Errorf("Apply(%q) = %q, want %q", tc.In, got, tc.Out)
		}
	}
}
