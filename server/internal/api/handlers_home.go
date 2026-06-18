package api

import (
	"net/http"
	"strconv"

	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/recs"
)

// getHome returns the assembled recommendation feed.
//
// GET /api/v1/home
//
// Honors user filter prefs from query params (?hide_explicit=&hide_video=
// &hide_shorts=). Serves a fresh cached payload within the TTL; on a cache miss
// it builds synchronously and caches the result. If building fails but a stale
// cache exists, the stale payload is returned with stale=true (cold-start).
func (d *Deps) getHome(w http.ResponseWriter, r *http.Request) {
	if d.Recs == nil {
		jsonError(w, "recs_unavailable", http.StatusServiceUnavailable)
		return
	}
	userID := auth.UserIDFromCtx(r.Context())
	prefs := prefsFromQuery(r)

	home, fresh, found := d.Recs.GetHomeCached(r.Context(), userID, prefs)
	if found && fresh {
		jsonOK(w, home)
		return
	}

	built, err := d.Recs.BuildHome(r.Context(), userID, prefs)
	if err != nil {
		// Build failed; fall back to a stale cache if we have one.
		if found {
			jsonOK(w, home)
			return
		}
		d.Log.Error().Err(err).Msg("recs: build home")
		jsonError(w, "internal", http.StatusInternalServerError)
		return
	}

	// Cache the fresh build (best effort).
	if err := d.Recs.PutHomeCached(r.Context(), userID, prefs, built); err != nil {
		d.Log.Warn().Err(err).Msg("recs: cache home")
	}
	jsonOK(w, built)
}

// prefsFromQuery reads the filter toggles from the request query.
func prefsFromQuery(r *http.Request) recs.Prefs {
	return recs.Prefs{
		HideExplicit: boolParam(r, "hide_explicit"),
		HideVideo:    boolParam(r, "hide_video"),
		HideShorts:   boolParam(r, "hide_shorts"),
	}
}

func boolParam(r *http.Request, key string) bool {
	v, _ := strconv.ParseBool(r.URL.Query().Get(key))
	return v
}
