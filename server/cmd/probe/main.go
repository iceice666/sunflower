// server/cmd/probe/main.go
package main

import (
	"flag"
	"fmt"
	"os"
)

func main() {
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "usage: probe <command> [flags]")
		fmt.Fprintln(os.Stderr, "  innertube next --video-id=<id> [-o json|url]")
		fmt.Fprintln(os.Stderr, "  innertube home")
		fmt.Fprintln(os.Stderr, "  innertube search --query=<q>")
		fmt.Fprintln(os.Stderr, "  innertube cookies-set --file=<path>")
	}
	if len(os.Args) < 2 {
		flag.Usage()
		os.Exit(1)
	}
	switch os.Args[1] {
	case "innertube":
		runInnertube(os.Args[2:])
	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", os.Args[1])
		flag.Usage()
		os.Exit(1)
	}
}
