package innertube_test

import (
	"testing"

	"github.com/iceice666/sunflower/server/internal/innertube"
	"github.com/iceice666/sunflower/server/internal/innertube/models"
)

func TestBuildAndroidMusicContext(t *testing.T) {
	ctx := innertube.BuildAndroidMusicContext(models.Locale{HL: "en", GL: "US"})
	client, ok := ctx["context"].(map[string]any)["client"].(map[string]any)
	if !ok {
		t.Fatal("context.client missing")
	}
	if client["clientName"] != "ANDROID_MUSIC" {
		t.Errorf("clientName = %v, want ANDROID_MUSIC", client["clientName"])
	}
	if client["hl"] != "en" {
		t.Errorf("hl = %v, want en", client["hl"])
	}
}

func TestBuildWebRemixContext(t *testing.T) {
	ctx := innertube.BuildWebRemixContext(models.Locale{HL: "es", GL: "MX"})
	client, ok := ctx["context"].(map[string]any)["client"].(map[string]any)
	if !ok {
		t.Fatal("context.client missing")
	}
	if client["clientName"] != "WEB_REMIX" {
		t.Errorf("clientName = %v, want WEB_REMIX", client["clientName"])
	}
	if client["hl"] != "es" {
		t.Errorf("hl = %v, want es", client["hl"])
	}
	if client["gl"] != "MX" {
		t.Errorf("gl = %v, want MX", client["gl"])
	}
}

func TestAndroidMusicAPIKey(t *testing.T) {
	key := innertube.AndroidMusicAPIKey()
	if key == "" {
		t.Error("AndroidMusicAPIKey returned empty string")
	}
}

func TestWebRemixAPIKey(t *testing.T) {
	key := innertube.WebRemixAPIKey()
	if key == "" {
		t.Error("WebRemixAPIKey returned empty string")
	}
}
