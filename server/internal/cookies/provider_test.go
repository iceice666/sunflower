package cookies_test

import (
	"testing"

	"github.com/iceice666/sunflower/server/internal/cookies"
)

func cookieMap(t *testing.T, raw string) map[string]string {
	t.Helper()
	cs := cookies.ParseCookies([]byte(raw))
	m := make(map[string]string, len(cs))
	for _, c := range cs {
		m[c.Name] = c.Value
	}
	return m
}

func TestParseCookies_LabeledExport(t *testing.T) {
	raw := "***INNERTUBE COOKIE*** =SID=abc; HSID=def; SAPISID=ghi\n" +
		"***VISITOR DATA*** =CgtX\n" +
		"***DATASYNC ID*** =123\n"
	m := cookieMap(t, raw)
	if len(m) != 3 {
		t.Fatalf("got %d cookies, want 3: %v", len(m), m)
	}
	if m["SID"] != "abc" || m["HSID"] != "def" || m["SAPISID"] != "ghi" {
		t.Errorf("unexpected values: %v", m)
	}
}

func TestParseCookies_RawHeader(t *testing.T) {
	m := cookieMap(t, "SID=abc; __Secure-3PSID=xyz")
	if len(m) != 2 || m["SID"] != "abc" || m["__Secure-3PSID"] != "xyz" {
		t.Errorf("unexpected: %v", m)
	}
}

func TestParseCookies_Netscape(t *testing.T) {
	raw := "# Netscape HTTP Cookie File\n" +
		".youtube.com\tTRUE\t/\tTRUE\t1999999999\tSID\tabc\n" +
		".youtube.com\tTRUE\t/\tTRUE\t1999999999\tHSID\tdef\n"
	m := cookieMap(t, raw)
	if len(m) != 2 || m["SID"] != "abc" || m["HSID"] != "def" {
		t.Errorf("unexpected: %v", m)
	}
}

func TestParseCookies_EmptyAndJunk(t *testing.T) {
	for _, raw := range []string{"", "   \n  ", "no-cookies-here"} {
		if cs := cookies.ParseCookies([]byte(raw)); cs != nil {
			t.Errorf("ParseCookies(%q) = %v, want nil", raw, cs)
		}
	}
}
