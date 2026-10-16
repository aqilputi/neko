package config

import (
	"errors"
	"fmt"
	"time"
)

// Lua defaults (plan §5.1, §5.3).
const (
	defaultLuaCommandMS  = 5000
	defaultLuaCallbackMS = 50
)

// Lua is the [lua] table: the scripting runtime (docs/lua.md).
type Lua struct {
	// Enabled loads init.lua and plugins/ (default true).
	Enabled bool `toml:"enabled"`
	// CommandTimeoutMS bounds a command, a file load and :lua (default 5000).
	CommandTimeoutMS int `toml:"command_timeout_ms"`
	// CallbackTimeoutMS bounds nk.on handlers and key functions (default 50).
	CallbackTimeoutMS int `toml:"callback_timeout_ms"`
	// AutoReload reloads when init.lua or plugins/ change (default false).
	AutoReload bool `toml:"auto_reload"`
}

func defaultLua() Lua {
	return Lua{Enabled: true, CommandTimeoutMS: defaultLuaCommandMS, CallbackTimeoutMS: defaultLuaCallbackMS}
}

// CommandTimeout is CommandTimeoutMS as a duration (the default if unset).
func (l Lua) CommandTimeout() time.Duration {
	return ms(l.CommandTimeoutMS, defaultLuaCommandMS)
}

// CallbackTimeout is CallbackTimeoutMS as a duration (the default if unset).
func (l Lua) CallbackTimeout() time.Duration {
	return ms(l.CallbackTimeoutMS, defaultLuaCallbackMS)
}

func ms(v, def int) time.Duration {
	if v <= 0 {
		v = def
	}
	return time.Duration(v) * time.Millisecond
}

// valid replaces each budget that is not positive by its default and says
// which ones it replaced.
func (l Lua) valid() (Lua, error) {
	var errs []error
	if l.CommandTimeoutMS <= 0 {
		errs = append(errs, fmt.Errorf("[lua] command_timeout_ms = %d: precisa ser > 0 (usando %d)", l.CommandTimeoutMS, defaultLuaCommandMS))
		l.CommandTimeoutMS = defaultLuaCommandMS
	}
	if l.CallbackTimeoutMS <= 0 {
		errs = append(errs, fmt.Errorf("[lua] callback_timeout_ms = %d: precisa ser > 0 (usando %d)", l.CallbackTimeoutMS, defaultLuaCallbackMS))
		l.CallbackTimeoutMS = defaultLuaCallbackMS
	}
	return l, errors.Join(errs...)
}
