package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"strings"
	"syscall"
	"text/tabwriter"

	"github.com/aqilputi/nekuroddo/internal/rpc"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// runRemote is nk run: one command on a running core (nk serve or
// nk --listen) over its socket. No command lists them. A command that asks
// first (Result.Pending) asks here, y/N on stdin. Exit: 0 ok, 1 error or
// declined, 2 usage.
func runRemote(ctx context.Context, args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	fs := newFlags("nk run", stderr)
	addr := fs.String("addr", "unix://"+defaultSocket(), "endereço do core: unix:///caminho ou tcp://127.0.0.1:porta")
	token := fs.String("token", "", "token do TCP (padrão: o gravado por nk serve em "+defaultToken()+")")
	session := fs.String("session", "", "a sessão em que o comando age (id)")
	yes := fs.Bool("yes", false, "confirma sem perguntar")
	pos, err := parseInterleaved(fs, args)
	if err != nil {
		return 2
	}
	sel := proto.Context{SessionID: *session}
	var name string
	cmdArgs := map[string]string{}
	if len(pos) > 0 {
		name = pos[0]
		for _, kv := range pos[1:] {
			k, v, ok := strings.Cut(kv, "=")
			if !ok || k == "" {
				fmt.Fprintf(stderr, "nk run: argumento %q: use nome=valor\n", kv)
				return 2
			}
			cmdArgs[k] = v
		}
	}
	tok := *token
	if tok == "" && strings.HasPrefix(*addr, "tcp://") {
		if tok, err = rpc.ReadTokenFile(defaultToken()); err != nil {
			fmt.Fprintf(stderr, "nk run: sem o token de %s: %v\n", *addr, err)
			return 1
		}
	}
	c, err := rpc.Dial(*addr, tok)
	if err != nil {
		fmt.Fprintln(stderr, "nk run:", err)
		return 1
	}
	defer c.Close()
	fail := func(err error) int {
		if errors.Is(err, syscall.ENOENT) || errors.Is(err, syscall.ECONNREFUSED) || isDialErr(err) {
			fmt.Fprintf(stderr, "nk run: nenhum core em %s (rode nk serve ou nk --listen): %v\n", *addr, err)
		} else {
			fmt.Fprintln(stderr, "nk run:", err)
		}
		return 1
	}
	if name == "" {
		cmds, err := c.Commands(ctx, sel)
		if err != nil {
			return fail(err)
		}
		tw := tabwriter.NewWriter(stdout, 0, 4, 2, ' ', 0)
		for _, x := range cmds {
			mark := " "
			if !x.Available {
				mark = "-" // not available for this selection
			}
			fmt.Fprintf(tw, "%s %s\t%s\t%s\n", mark, x.Name, x.Scope, x.Title)
		}
		_ = tw.Flush()
		return 0
	}
	res, err := c.Run(ctx, name, cmdArgs, sel)
	if err != nil {
		return fail(err)
	}
	if res.Pending != nil {
		ok := *yes
		if !ok {
			fmt.Fprintf(stderr, "%s [y/N] ", res.Pending.Question)
			line, _ := bufio.NewReader(stdin).ReadString('\n')
			a := strings.ToLower(strings.TrimSpace(line))
			ok = a == "y" || a == "s" || a == "yes" || a == "sim"
		}
		res, err = c.Confirm(ctx, res.Pending.Token, ok)
		if err != nil {
			return fail(err)
		}
		if !ok {
			fmt.Fprintln(stderr, "cancelado")
			return 1
		}
	}
	return printResult(stdout, stderr, res)
}

// isDialErr is a failure to reach the server at all.
func isDialErr(err error) bool {
	var op *net.OpError
	return errors.As(err, &op) && op.Op == "dial"
}

// printResult shows a Result: the toast (an error one on stderr, exit 1),
// the terminal a client should take over, and the data as JSON.
func printResult(stdout, stderr io.Writer, res proto.Result) int {
	code := 0
	if res.Toast != "" {
		if res.Err {
			fmt.Fprintln(stderr, res.Toast)
			code = 1
		} else {
			fmt.Fprintln(stdout, res.Toast)
		}
	}
	if res.Exec != nil {
		fmt.Fprintf(stdout, "rode: %s\n", strings.Join(res.Exec.Argv, " "))
	}
	if d := bytes.TrimSpace(res.Data); len(d) > 0 && string(d) != "null" {
		var buf bytes.Buffer
		if json.Indent(&buf, d, "", "  ") != nil {
			buf.Reset()
			buf.Write(d)
		}
		fmt.Fprintln(stdout, buf.String())
	}
	return code
}

// parseInterleaved parses fs's flags wherever they are among the
// positional arguments (flag stops at the first one), returning those.
func parseInterleaved(fs *flag.FlagSet, args []string) ([]string, error) {
	var pos []string
	for {
		if err := fs.Parse(args); err != nil {
			return nil, err
		}
		if fs.NArg() == 0 {
			return pos, nil
		}
		pos = append(pos, fs.Arg(0))
		args = fs.Args()[1:]
	}
}
