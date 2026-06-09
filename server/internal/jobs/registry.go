// Package jobs provides a simple in-memory job registry for background tasks.
package jobs

import (
	"fmt"
	"sync"
	"time"

	"github.com/google/uuid"
)

// Status represents a job's lifecycle state.
type Status string

const (
	StatusPending   Status = "pending"
	StatusRunning   Status = "running"
	StatusCompleted Status = "completed"
	StatusFailed    Status = "failed"
)

// Job holds runtime state for a background task.
type Job struct {
	ID             string    `json:"id"`
	Status         Status    `json:"status"`
	ProcessedFiles int       `json:"processed_files"`
	Error          string    `json:"error,omitempty"`
	CreatedAt      time.Time `json:"created_at"`
	UpdatedAt      time.Time `json:"updated_at"`
}

// Registry is a thread-safe in-memory store for Jobs.
type Registry struct {
	mu   sync.RWMutex
	jobs map[string]*Job
}

// NewRegistry returns an empty Registry.
func NewRegistry() *Registry {
	return &Registry{jobs: make(map[string]*Job)}
}

// Create inserts a new pending job and returns it.
func (r *Registry) Create() *Job {
	j := &Job{
		ID:        uuid.New().String(),
		Status:    StatusPending,
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
	}
	r.mu.Lock()
	r.jobs[j.ID] = j
	r.mu.Unlock()
	return j
}

// Get returns the job with the given id and a boolean indicating whether it was found.
func (r *Registry) Get(id string) (*Job, bool) {
	r.mu.RLock()
	j, ok := r.jobs[id]
	r.mu.RUnlock()
	return j, ok
}

// Update applies fn to the job identified by id under a write lock.
func (r *Registry) Update(id string, fn func(*Job)) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	j, ok := r.jobs[id]
	if !ok {
		return fmt.Errorf("job %s not found", id)
	}
	fn(j)
	j.UpdatedAt = time.Now()
	return nil
}
