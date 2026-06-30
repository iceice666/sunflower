package adminui

import (
	"embed"
	"html/template"
	"io/fs"
	"net/http"
)

//go:embed templates/*.html static/*
var files embed.FS

func Parse(page string, funcs template.FuncMap) (*template.Template, error) {
	return template.New("layout.html").Funcs(funcs).ParseFS(files, "templates/layout.html", "templates/"+page)
}

func StaticHandler(w http.ResponseWriter, r *http.Request) {
	staticFS, err := fs.Sub(files, "static")
	if err != nil {
		http.NotFound(w, r)
		return
	}
	http.StripPrefix("/admin/static/", http.FileServer(http.FS(staticFS))).ServeHTTP(w, r)
}
