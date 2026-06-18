package sig

import (
	"fmt"
	"net/url"
	"regexp"

	"github.com/dop251/goja"
)

var (
	// nsigNameRe extracts the nsig function name from base.js.
	// YT obfuscation changes the name but the pattern around it is stable.
	nsigNameRe = regexp.MustCompile(`\.get\("n"\)\)&&\(b=([a-zA-Z0-9$]+)\[`)
)

type nsigEntry struct {
	prog     *goja.Program
	funcName string
}

func extractNsig(js string) (*nsigEntry, error) {
	m := nsigNameRe.FindStringSubmatch(js)
	if m == nil {
		return nil, fmt.Errorf("nsig: function name not found in base.js")
	}
	// The name may be an array access like Xyz[0]; extract the array name.
	arrayName := m[1]

	// Find the array declaration to get the actual function.
	arrayRe := regexp.MustCompile(`var ` + regexp.QuoteMeta(arrayName) + `\s*=\s*\[([^\]]+)\]`)
	am := arrayRe.FindStringSubmatch(js)

	var funcName string
	if am != nil {
		// The array contains function names; use the first element.
		funcName = am[1]
	} else {
		funcName = arrayName
	}

	// Find the function body by name.
	funcRe := regexp.MustCompile(`(?:var |,)\s*` + regexp.QuoteMeta(funcName) + `\s*=\s*(function\([^)]*\)\s*\{[\s\S]*?\})\s*[,;]`)
	fm := funcRe.FindStringSubmatch(js)
	if fm == nil {
		return nil, fmt.Errorf("nsig: function body not found for %q", funcName)
	}

	src := "var " + funcName + "=" + fm[1]
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
