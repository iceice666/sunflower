package api

import (
	"net/http"

	"github.com/go-chi/chi/v5"
)

func (d *Deps) getJob(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "id")
	job, ok := d.Jobs.Get(id)
	if !ok {
		jsonError(w, "not_found", http.StatusNotFound)
		return
	}
	jsonOK(w, job)
}
