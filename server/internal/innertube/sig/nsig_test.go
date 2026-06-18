package sig

import "testing"

const syntheticBaseJS = `
var abc=function(a){a.get("n"))&&(b=xyz[0](a.get("n")));};
var xyz=[nfunc];
var nfunc=function(a){if(a.length){return a.split("").reverse().join("")}return a};
`

func TestExtractNsig(t *testing.T) {
	e, err := extractNsig(syntheticBaseJS)
	if err != nil {
		t.Fatalf("extractNsig: %v", err)
	}
	got, err := e.decode("hello")
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	want := "olleh"
	if got != want {
		t.Errorf("decode(%q) = %q, want %q", "hello", got, want)
	}
}

func TestExtractNsigDeclaration(t *testing.T) {
	// Test function declaration form (not expression)
	const declJS = `
var abc=function(a){a.get("n"))&&(b=xyz[0](a.get("n")));};
var xyz=[nfunc2];
function nfunc2(a){return a+"_ok"}
`
	e, err := extractNsig(declJS)
	if err != nil {
		t.Fatalf("extractNsig(decl): %v", err)
	}
	got, err := e.decode("test")
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	if got != "test_ok" {
		t.Errorf("decode = %q, want %q", got, "test_ok")
	}
}

func TestExtractBody(t *testing.T) {
	tests := []struct {
		name   string
		js     string
		start  int
		want   string
		wantOK bool
	}{
		{
			name:   "simple function",
			js:     `{return a}`,
			start:  0,
			want:   `{return a}`,
			wantOK: true,
		},
		{
			name:   "nested if braces",
			js:     `{if(a){if(b){return 1;}}return 0}`,
			start:  0,
			want:   `{if(a){if(b){return 1;}}return 0}`,
			wantOK: true,
		},
		{
			name:   "brace inside double-quoted string literal",
			js:     `{var s="}fake";return s}`,
			start:  0,
			want:   `{var s="}fake";return s}`,
			wantOK: true,
		},
		{
			name:   "brace inside single-quoted string literal",
			js:     `{var s='}fake';return s}`,
			start:  0,
			want:   `{var s='}fake';return s}`,
			wantOK: true,
		},
		{
			name:   "escaped quote inside string does not close early",
			js:     `{var s="a\"b}c";return s}`,
			start:  0,
			want:   `{var s="a\"b}c";return s}`,
			wantOK: true,
		},
		{
			name:   "unbalanced braces returns false",
			js:     `{if(a){return 1;}`,
			start:  0,
			want:   "",
			wantOK: false,
		},
		{
			name:   "non-zero start offset",
			js:     `prefix{return x}suffix`,
			start:  6,
			want:   `{return x}`,
			wantOK: true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := extractBody(tc.js, tc.start)
			if ok != tc.wantOK {
				t.Errorf("extractBody ok=%v, want %v", ok, tc.wantOK)
			}
			if got != tc.want {
				t.Errorf("extractBody = %q, want %q", got, tc.want)
			}
		})
	}
}
