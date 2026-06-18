package events

import "testing"

func TestQualifies(t *testing.T) {
	cases := []struct {
		name        string
		played, dur int
		want        bool
	}{
		{"over absolute floor", 31_000, 240_000, true},
		{"under floor, long track", 10_000, 240_000, false},
		{"half of short track", 20_000, 40_000, true}, // 20s >= 50% of 40s
		{"under half of short track", 5_000, 40_000, false},
		{"exact floor", 30_000, 0, true},
		{"unknown duration under floor", 5_000, 0, false},
		{"zero played", 0, 240_000, false},
	}
	for _, c := range cases {
		if got := Qualifies(c.played, c.dur); got != c.want {
			t.Errorf("%s: Qualifies(%d,%d)=%v want %v", c.name, c.played, c.dur, got, c.want)
		}
	}
}
