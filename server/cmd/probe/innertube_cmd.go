// server/cmd/probe/innertube_cmd.go
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"github.com/iceice666/sunflower/server/internal/cookies"
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
	case "cookies-set":
		runCookiesSet(args[1:])
	default:
		fmt.Fprintf(os.Stderr, "unknown innertube subcommand: %s\n", args[0])
		os.Exit(1)
	}
}

func runNext(args []string) {
	fs := flag.NewFlagSet("next", flag.ExitOnError)
	videoID := fs.String("video-id", "", "YouTube video ID (required)")
	output := fs.String("o", "json", "output format: json|url")
	dumpRaw := fs.String("dump-raw", "", "write raw /next JSON to this file (e.g. for fixture capture)")
	cookieFile := fs.String("cookie-file", "", "path to YT cookie jar (labeled export / Cookie header / Netscape cookies.txt)")
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

	opts := innertube.ClientOpts{
		SigCache: cache,
		Locale:   models.Locale{HL: "en", GL: "US"},
	}
	if *cookieFile != "" {
		raw, err := os.ReadFile(*cookieFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "read cookie file: %v\n", err)
			os.Exit(1)
		}
		cks := cookies.ParseCookies(raw)
		if len(cks) == 0 {
			fmt.Fprintln(os.Stderr, "warning: no cookies parsed from --cookie-file")
		} else {
			fmt.Fprintf(os.Stderr, "loaded %d cookies from %s\n", len(cks), *cookieFile)
		}
		opts.Cookies = func() []*http.Cookie { return cks }
	}
	client := innertube.NewClient(opts)

	playerResp, err := client.Player(ctx, *videoID)
	if err != nil {
		fmt.Fprintf(os.Stderr, "player: %v\n", err)
		os.Exit(1)
	}
	if playerResp.NsigErr != nil {
		fmt.Fprintf(os.Stderr, "warning: nsig decode failed — stream URL may be throttled: %v\n", playerResp.NsigErr)
	}

	nextRaw, err := client.Next(ctx, *videoID, nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "next: %v\n", err)
		os.Exit(1)
	}

	// Write raw response only when explicitly requested (avoids overwriting committed fixtures).
	if *dumpRaw != "" {
		if err := os.WriteFile(*dumpRaw, nextRaw, 0644); err != nil {
			fmt.Fprintf(os.Stderr, "warning: dump-raw write: %v\n", err)
		}
	}

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

	// Guest mode (no cookies wired): the home feed degrades to generic,
	// non-personalized content. Surface this so the output isn't mistaken
	// for a personalized feed.
	fmt.Fprintln(os.Stderr, "warning: no cookies configured — home feed degrades to generic content")

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

func runCookiesSet(args []string) {
	fs := flag.NewFlagSet("cookies-set", flag.ExitOnError)
	file := fs.String("file", "", "path to Netscape-format cookie file (required)")
	serverURL := fs.String("server", "http://localhost:8080", "sunflowerd base URL")
	token := fs.String("token", "", "device token (required)")
	fs.Parse(args)

	if *file == "" || *token == "" {
		fmt.Fprintln(os.Stderr, "--file and --token are required")
		os.Exit(1)
	}

	raw, err := os.ReadFile(*file)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read file: %v\n", err)
		os.Exit(1)
	}

	body, _ := json.Marshal(map[string]string{"cookies": string(raw)})
	req, _ := http.NewRequest(http.MethodPost, *serverURL+"/api/v1/cookies/youtube", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+*token)

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "upload: %v\n", err)
		os.Exit(1)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNoContent {
		fmt.Println("cookies uploaded successfully")
	} else {
		io.Copy(os.Stderr, resp.Body)
		os.Exit(1)
	}
}
