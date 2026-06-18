package sig

import (
	"fmt"
	"net/url"
	"regexp"

	"github.com/dop251/goja"
)

var (
	// nsigNameRes is a priority list of patterns to extract the nsig function name
	// from base.js. YT obfuscation changes the surrounding pattern across versions,
	// so we try each in order and stop at the first match.
	nsigNameRes = []*regexp.Regexp{
		// Most common: array access pattern  b=XYZ[
		regexp.MustCompile(`\.get\("n"\)\)&&\(b=([a-zA-Z0-9$]+)\[`),
		// Fallback 1: direct call instead of array access  b=XYZ(
		regexp.MustCompile(`\.get\("n"\)\)&&\(b=([a-zA-Z0-9$]+)\(`),
		// Fallback 2: wider match for array-index pattern
		regexp.MustCompile(`[a-zA-Z0-9$]{2,}\s*=\s*([a-zA-Z0-9$]{2,})\[0\]\s*\(`),
	}
)

type nsigEntry struct {
	prog     *goja.Program
	funcName string
}

// extractBody returns the balanced brace-delimited body starting at position
// start in js, where js[start] == '{'. It skips string literals to avoid
// counting braces inside them.
func extractBody(js string, start int) (string, bool) {
	depth := 0
	inStr := byte(0) // 0 = not in string; '"', '\'', '`' = inside that string
	escaped := false
	for i := start; i < len(js); i++ {
		ch := js[i]
		if escaped {
			escaped = false
			continue
		}
		if ch == '\\' && inStr != 0 {
			escaped = true
			continue
		}
		if inStr != 0 {
			if ch == inStr {
				inStr = 0
			}
			continue
		}
		switch ch {
		case '"', '\'', '`':
			inStr = ch
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return js[start : i+1], true
			}
		}
	}
	return "", false
}

func extractNsig(js string) (*nsigEntry, error) {
	// Try each name pattern in priority order.
	var m []string
	for _, re := range nsigNameRes {
		m = re.FindStringSubmatch(js)
		if m != nil {
			break
		}
	}
	if m == nil {
		return nil, fmt.Errorf("nsig: function name not found in base.js")
	}
	// The name may be an array access like Xyz[0]; extract the array name.
	arrayName := m[1]

	// Find the array declaration to get the actual function name.
	arrayRe := regexp.MustCompile(`var ` + regexp.QuoteMeta(arrayName) + `\s*=\s*\[([^\]]+)\]`)
	am := arrayRe.FindStringSubmatch(js)

	var funcName string
	if am != nil {
		// The array contains function names; use the first element.
		funcName = am[1]
	} else {
		funcName = arrayName
	}

	// Find the function start position using a regex, then extract the body
	// with brace-counting so deeply nested functions are handled correctly.
	funcStartRe := regexp.MustCompile(`(?:var |,)\s*` + regexp.QuoteMeta(funcName) + `\s*=\s*(function\([^)]*\)\s*)\{`)
	loc := funcStartRe.FindStringSubmatchIndex(js)
	if loc == nil {
		return nil, fmt.Errorf("nsig: function body not found for %q", funcName)
	}
	// loc[2]:loc[3] is the capture group — "function(...) " without the opening brace.
	sigPart := js[loc[2]:loc[3]]
	// loc[1]-1 is the index of the opening '{' (the last char of the full match).
	braceStart := loc[1] - 1
	body, ok := extractBody(js, braceStart)
	if !ok {
		return nil, fmt.Errorf("nsig: unbalanced braces in function body for %q", funcName)
	}

	src := "var " + funcName + "=" + sigPart + body
	prog, err := goja.Compile("nsig", src, false)
	if err != nil {
		return nil, fmt.Errorf("nsig: compile: %w", err)
	}
	return &nsigEntry{prog: prog, funcName: funcName}, nil
}

func (e *nsigEntry) decode(token string) (string, error) {
	vm := goja.New()
	if _, err := vm.RunProgram(e.prog); err != nil {
		return "", fmt.Errorf("nsig: init runtime: %w", err)
	}
	fn, ok := goja.AssertFunction(vm.Get(e.funcName))
	if !ok {
		return "", fmt.Errorf("nsig: %q is not a function", e.funcName)
	}
	result, err := fn(goja.Undefined(), vm.ToValue(token))
	if err != nil {
		return "", fmt.Errorf("nsig: execute: %w", err)
	}
	return result.String(), nil
}

func parseAndReplaceN(rawURL string, nsig *nsigEntry) (string, error) {
	u, err := url.Parse(rawURL)
	if err != nil {
		return rawURL, fmt.Errorf("nsig: parse url: %w", err)
	}
	q := u.Query()
	n := q.Get("n")
	if n == "" {
		return rawURL, nil // no n param, nothing to do
	}
	decoded, err := nsig.decode(n)
	if err != nil {
		return rawURL, err
	}
	q.Set("n", decoded)
	u.RawQuery = q.Encode()
	return u.String(), nil
}
