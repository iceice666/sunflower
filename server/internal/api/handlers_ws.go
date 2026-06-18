package api

import (
	"encoding/json"
	"net/http"

	"github.com/iceice666/sunflower/server/internal/auth"
	"github.com/iceice666/sunflower/server/internal/ws"
)

// wsNowPlaying upgrades the connection to the now-playing WebSocket. The device
// auth middleware has already validated the Bearer token and injected the device
// id; the upgraded socket is bound to that device.
//
// GET /api/v1/ws/now-playing  (subprotocol sunflower.now-playing.v1)
func (d *Deps) wsNowPlaying(w http.ResponseWriter, r *http.Request) {
	if d.Hub == nil {
		jsonError(w, "ws_unavailable", http.StatusServiceUnavailable)
		return
	}
	deviceID := auth.DeviceIDFromCtx(r.Context()).String()
	if err := ws.Serve(d.Hub, deviceID, w, r); err != nil {
		// Upgrade failures already wrote a response; just log.
		d.Log.Warn().Err(err).Msg("ws: upgrade")
	}
}

// wsCommandRequest targets a device with a control command.
type wsCommandRequest struct {
	DeviceID string `json:"device_id"`
	Command  string `json:"command"` // pause | play | skip_next | skip_prev
}

// wsCommand routes a control command to a device's now-playing connections. The
// admin/controller posts here; the hub forwards over the WebSocket.
//
// POST /api/v1/ws/command
func (d *Deps) wsCommand(w http.ResponseWriter, r *http.Request) {
	if d.Hub == nil {
		jsonError(w, "ws_unavailable", http.StatusServiceUnavailable)
		return
	}
	var req wsCommandRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.DeviceID == "" || req.Command == "" {
		jsonError(w, "invalid_request", http.StatusBadRequest)
		return
	}
	switch req.Command {
	case ws.CmdPause, ws.CmdPlay, ws.CmdSkipNext, ws.CmdSkipPrev:
	default:
		jsonError(w, "invalid_command", http.StatusBadRequest)
		return
	}
	n := d.Hub.SendCommand(req.DeviceID, req.Command)
	jsonOK(w, map[string]int{"delivered": n})
}

// adminDashboard returns a JSON snapshot of now-playing per device plus cookie
// health — the v1 admin surface (no UI).
//
// GET /api/v1/admin
func (d *Deps) adminDashboard(w http.ResponseWriter, r *http.Request) {
	resp := map[string]any{}
	if d.Hub != nil {
		resp["now_playing"] = d.Hub.Snapshot()
	} else {
		resp["now_playing"] = []any{}
	}

	// Cookie health (best effort; absent table → unknown).
	var status string
	if d.DB != nil {
		_ = d.DB.QueryRow(r.Context(),
			`SELECT status FROM cookie_health WHERE provider='youtube'`).Scan(&status)
	}
	if status == "" {
		status = "unknown"
	}
	resp["cookie_status"] = status

	jsonOK(w, resp)
}
