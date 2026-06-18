// server/cmd/probe/innertube_cmd.go
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"os"
	"time"

	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
	"github.com/iceice666/sunflower/server/internal/innertube/parser"
	"github.com/iceice666/sunflower/server/internal/innertube/sig"
)

func runInnertube(args []string) {
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "usage: probe innertube <next|home|search|cookies-set>")
		os.Exit(1)
	}
	switch args[0] {
	case "next":
		runNext(args[1:])
	case "home":
		runHome(args[1:])
	case "search":
		runSearch(args[1:])
	default:
		fmt.Fprintf(os.Stderr, "unknown innertube subcommand: %s\n", args[0])
		os.Exit(1)
	}
}

func runNext(args []string) {
	fs := flag.NewFlagSet("next", flag.ExitOnError)
	videoID := fs.String("video-id", "", "YouTube video ID (required)")
	output := fs.String("o", "json", "output format: json|url")
	fs.Parse(args)

	if *videoID == "" {
		fmt.Fprintln(os.Stderr, "--video-id is required")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}

	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	playerResp, err := client.Player(ctx, *videoID)
	if err != nil {
		fmt.Fprintf(os.Stderr, "player: %v\n", err)
		os.Exit(1)
	}

	nextRaw, err := client.Next(ctx, *videoID, nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "next: %v\n", err)
		os.Exit(1)
	}

	// Save fixture for parser tests.
	_ = os.MkdirAll("server/internal/innertube/parser/testdata", 0755)
	_ = os.WriteFile("server/internal/innertube/parser/testdata/next_response.json", nextRaw, 0644)

	nextPage := parser.ParseNextPage(nextRaw)
	result := models.ProbeNextResult{
		CurrentURL:   playerResp.Stream.URL,
		ExpiresAt:    playerResp.Stream.ExpiresAt,
		Itag:         playerResp.Stream.Itag,
		NextItems:    nextPage.Related,
		Continuation: nextPage.Continuation,
	}

	switch *output {
	case "url":
		fmt.Println(result.CurrentURL)
	default:
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		enc.Encode(result)
	}
}

func runHome(_ []string) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}
	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	raw, err := client.Browse(ctx, "FEmusic_home", nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "browse: %v\n", err)
		os.Exit(1)
	}
	page := parser.ParseHomePage(raw)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(page)
}

func runSearch(args []string) {
	fs := flag.NewFlagSet("search", flag.ExitOnError)
	query := fs.String("query", "", "search query (required)")
	fs.Parse(args)
	if *query == "" {
		fmt.Fprintln(os.Stderr, "--query is required")
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	cache := sig.NewCache(http.DefaultClient)
	if err := cache.Bootstrap(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "sig bootstrap: %v\n", err)
		os.Exit(1)
	}
	client := innertube.NewClient(innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	})

	raw, err := client.Search(ctx, *query)
	if err != nil {
		fmt.Fprintf(os.Stderr, "search: %v\n", err)
		os.Exit(1)
	}
	page := parser.ParseSearchPage(raw)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(page)
}
