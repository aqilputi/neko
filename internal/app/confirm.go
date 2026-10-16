package app

import (
	"crypto/rand"
	"errors"
	"sort"
	"time"

	"github.com/aqilputi/nekuroddo/internal/core"
)

// ConfirmTTL is how long a pending confirmation waits for an answer.
const ConfirmTTL = 60 * time.Second

var (
	ErrUnknownToken = errors.New("confirmação desconhecida ou já respondida")
	ErrExpired      = errors.New("confirmação expirou")
)

// Outcomes of a confirmation, in confirm.resolved.
const (
	OutcomeYes     = "yes"
	OutcomeNo      = "no"
	OutcomeExpired = "expired"
)

// ConfirmResolved is the payload of confirm.resolved: every client closes
// the dialog for Token.
type ConfirmResolved struct {
	Token   string `json:"token"`
	Outcome string `json:"outcome"`
}

type pendingAction struct {
	info core.PendingConfirmation
	yes  func() (Result, error)
}

// Ask parks yes behind a single-use token and returns the Result that asks
// the client (any client) to confirm. confirm.requested is published so the
// other clients can show the same dialog.
func (a *App) Ask(question string, yes func() (Result, error)) Result {
	now := a.clock.Now()
	p := core.PendingConfirmation{Token: rand.Text(), Question: question, ExpiresAt: now.Add(ConfirmTTL)}
	a.cmu.Lock()
	expired := a.sweepLocked(now)
	a.pending[p.Token] = &pendingAction{info: p, yes: yes}
	a.cmu.Unlock()
	for _, tok := range expired {
		a.Publish(EvConfirmResolved, ConfirmResolved{Token: tok, Outcome: OutcomeExpired})
	}
	a.Publish(EvConfirmRequested, p)
	return Result{Pending: &p}
}

// sweepLocked drops expired confirmations and returns their tokens; a.cmu
// must be held.
func (a *App) sweepLocked(now time.Time) []string {
	var gone []string
	for tok, p := range a.pending {
		if !now.Before(p.info.ExpiresAt) {
			delete(a.pending, tok)
			gone = append(gone, tok)
		}
	}
	sort.Strings(gone)
	return gone
}

// take removes token and reports whether it was still valid.
func (a *App) take(token string) (*pendingAction, error) {
	a.cmu.Lock()
	p, ok := a.pending[token]
	delete(a.pending, token)
	a.cmu.Unlock()
	if !ok {
		return nil, ErrUnknownToken
	}
	if !a.clock.Now().Before(p.info.ExpiresAt) {
		a.Publish(EvConfirmResolved, ConfirmResolved{Token: token, Outcome: OutcomeExpired})
		return nil, ErrExpired
	}
	return p, nil
}

// Confirm answers yes: the parked action runs, once.
func (a *App) Confirm(token string) (Result, error) {
	p, err := a.take(token)
	if err != nil {
		return Result{}, err
	}
	a.Publish(EvConfirmResolved, ConfirmResolved{Token: token, Outcome: OutcomeYes})
	return p.yes()
}

// Cancel answers no: the parked action is dropped.
func (a *App) Cancel(token string) error {
	if _, err := a.take(token); err != nil {
		return err
	}
	a.Publish(EvConfirmResolved, ConfirmResolved{Token: token, Outcome: OutcomeNo})
	return nil
}

// Pending lists the confirmations still waiting, oldest first (a client that
// connects late shows them).
func (a *App) Pending() []core.PendingConfirmation {
	now := a.clock.Now()
	a.cmu.Lock()
	defer a.cmu.Unlock()
	var out []core.PendingConfirmation
	for _, p := range a.pending {
		if now.Before(p.info.ExpiresAt) {
			out = append(out, p.info)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ExpiresAt.Before(out[j].ExpiresAt) })
	return out
}
