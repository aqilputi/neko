// GET /v1/keymap and GET /v1/commands of `nk serve --demo`, captured from the
// real server (regenerate when the core's commands change).

import type { Binding, CommandInfo } from '../client/types.ts';

export const DEMO_KEYMAP: Binding[] = [
 {
  "command": "session.archive",
  "keys": [
   "X"
  ],
  "mode": "normal",
  "help": {
   "key": "X",
   "desc": "arquiva/restaura (no contexto: todas)"
  }
 },
 {
  "command": "session.stop",
  "keys": [
   "x"
  ],
  "mode": "normal",
  "help": {
   "key": "x",
   "desc": "parar a sessão em background"
  }
 },
 {
  "command": "session.remove",
  "keys": [
   "dd"
  ],
  "mode": "normal",
  "help": {
   "key": "dd",
   "desc": "remover a sessão em background"
  }
 },
 {
  "command": "session.rename",
  "keys": [
   "n"
  ],
  "mode": "normal",
  "help": {
   "key": "n",
   "desc": "renomear a sessão"
  }
 },
 {
  "command": "session.move",
  "keys": [
   "m"
  ],
  "mode": "normal",
  "help": {
   "key": "m",
   "desc": "mover a sessão para outro contexto"
  }
 },
 {
  "command": "session.new",
  "keys": [
   "c"
  ],
  "mode": "normal",
  "help": {
   "key": "c",
   "desc": "novo agent em background"
  }
 },
 {
  "command": "session.open",
  "keys": [
   "enter"
  ],
  "mode": "normal",
  "help": {
   "key": "enter",
   "desc": "abre e foca"
  }
 },
 {
  "command": "session.preview",
  "keys": [
   "o"
  ],
  "mode": "normal",
  "help": {
   "key": "o",
   "desc": "abre sem focar"
  }
 },
 {
  "command": "session.fullscreen",
  "keys": [
   "O"
  ],
  "mode": "normal",
  "help": {
   "key": "O",
   "desc": "tela cheia"
  }
 },
 {
  "command": "pane.close",
  "keys": [
   "w"
  ],
  "mode": "normal",
  "help": {
   "key": "w",
   "desc": "fecha painel"
  }
 },
 {
  "command": "pane.zoom",
  "keys": [
   "z"
  ],
  "mode": "normal",
  "help": {
   "key": "z",
   "desc": "zoom do painel"
  }
 },
 {
  "command": "todo.start",
  "keys": [
   "s"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "s",
   "desc": "começa a tarefa"
  }
 },
 {
  "command": "todo.done",
  "keys": [
   "D"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "D",
   "desc": "marca a tarefa como done"
  }
 },
 {
  "command": "jira.toggle",
  "keys": [
   "J"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "J",
   "desc": "todo↔jira: issue vira tarefa, tarefa vira issue"
  }
 },
 {
  "command": "jira.fetch",
  "keys": [
   "F"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "F",
   "desc": "busca as issues no Jira (via claude, ~$0.1)"
  }
 },
 {
  "command": "jira.open_url",
  "keys": [
   "o"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "o",
   "desc": "abre a issue no browser"
  }
 },
 {
  "command": "nav.down",
  "keys": [
   "j",
   "down"
  ],
  "mode": "normal",
  "help": {
   "key": "j/down",
   "desc": "desce"
  }
 },
 {
  "command": "nav.up",
  "keys": [
   "k",
   "up"
  ],
  "mode": "normal",
  "help": {
   "key": "k/up",
   "desc": "sobe"
  }
 },
 {
  "command": "nav.top",
  "keys": [
   "gg"
  ],
  "mode": "normal",
  "help": {
   "key": "gg",
   "desc": "topo"
  }
 },
 {
  "command": "nav.bottom",
  "keys": [
   "G"
  ],
  "mode": "normal",
  "help": {
   "key": "G",
   "desc": "fim"
  }
 },
 {
  "command": "nav.half_down",
  "keys": [
   "ctrl+d"
  ],
  "mode": "normal",
  "help": {
   "key": "ctrl+d",
   "desc": "meia página ↓"
  }
 },
 {
  "command": "nav.half_up",
  "keys": [
   "ctrl+u"
  ],
  "mode": "normal",
  "help": {
   "key": "ctrl+u",
   "desc": "meia página ↑"
  }
 },
 {
  "command": "nav.page_down",
  "keys": [
   "ctrl+f",
   "pgdown"
  ],
  "mode": "normal",
  "help": {
   "key": "ctrl+f/pgdown",
   "desc": "página ↓"
  }
 },
 {
  "command": "nav.page_up",
  "keys": [
   "ctrl+b",
   "pgup"
  ],
  "mode": "normal",
  "help": {
   "key": "ctrl+b/pgup",
   "desc": "página ↑"
  }
 },
 {
  "command": "nav.collapse",
  "keys": [
   "h",
   "left"
  ],
  "mode": "normal",
  "help": {
   "key": "h/left",
   "desc": "fecha nó"
  }
 },
 {
  "command": "nav.expand",
  "keys": [
   "l",
   "right"
  ],
  "mode": "normal",
  "help": {
   "key": "l/right",
   "desc": "abre nó"
  }
 },
 {
  "command": "nav.pick",
  "keys": [
   "space"
  ],
  "mode": "normal",
  "help": {
   "key": "space",
   "desc": "seleciona (p/ :group); no contexto: alterna"
  }
 },
 {
  "command": "nav.next_attention",
  "keys": [
   "a"
  ],
  "mode": "normal",
  "help": {
   "key": "a",
   "desc": "próxima te esperando"
  }
 },
 {
  "command": "nav.prev_attention",
  "keys": [
   "A"
  ],
  "mode": "normal",
  "help": {
   "key": "A",
   "desc": "anterior te esperando"
  }
 },
 {
  "command": "session.insert",
  "keys": [
   "i"
  ],
  "mode": "normal",
  "help": {
   "key": "i",
   "desc": "INSERT no painel"
  }
 },
 {
  "command": "tab.sessions",
  "keys": [
   "1"
  ],
  "mode": "normal",
  "help": {
   "key": "1",
   "desc": "sessões"
  }
 },
 {
  "command": "tab.inbox",
  "keys": [
   "2"
  ],
  "mode": "normal",
  "help": {
   "key": "2",
   "desc": "inbox"
  }
 },
 {
  "command": "tab.todos",
  "keys": [
   "3"
  ],
  "mode": "normal",
  "help": {
   "key": "3",
   "desc": "todos + jira"
  }
 },
 {
  "command": "tab.archive",
  "keys": [
   "4"
  ],
  "mode": "normal",
  "help": {
   "key": "4",
   "desc": "arquivo"
  }
 },
 {
  "command": "tab.next",
  "keys": [
   "tab"
  ],
  "mode": "normal",
  "help": {
   "key": "tab",
   "desc": "próxima aba"
  }
 },
 {
  "command": "tab.prev",
  "keys": [
   "shift+tab"
  ],
  "mode": "normal",
  "help": {
   "key": "shift+tab",
   "desc": "aba anterior"
  }
 },
 {
  "command": "view.filter",
  "keys": [
   "/"
  ],
  "mode": "normal",
  "help": {
   "key": "/",
   "desc": "filtro fuzzy"
  }
 },
 {
  "command": "view.show_all",
  "keys": [
   "H"
  ],
  "mode": "normal",
  "help": {
   "key": "H",
   "desc": "mostra paradas/arquivadas/subagents concluídos"
  }
 },
 {
  "command": "view.sort",
  "keys": [
   "S"
  ],
  "mode": "normal",
  "help": {
   "key": "S",
   "desc": "ordem: atividade/custo/nome"
  }
 },
 {
  "command": "view.theme",
  "keys": [
   "T"
  ],
  "mode": "normal",
  "help": {
   "key": "T",
   "desc": "tema"
  }
 },
 {
  "command": "switcher.open",
  "keys": [
   "ctrl+k"
  ],
  "mode": "*",
  "help": {
   "key": "ctrl+k",
   "desc": "ir p/ chat rodando/esperando"
  }
 },
 {
  "command": "palette.open",
  "keys": [
   ":",
   "ctrl+p"
  ],
  "mode": "normal",
  "help": {
   "key": ":/ctrl+p",
   "desc": "comandos"
  }
 },
 {
  "command": "usage.show",
  "keys": [
   "u"
  ],
  "mode": "normal",
  "help": {
   "key": "u",
   "desc": "uso (/usage)"
  }
 },
 {
  "command": "help.open",
  "keys": [
   "ctrl+/",
   "?",
   "ctrl+_"
  ],
  "mode": "normal",
  "help": {
   "key": "ctrl+//?/ctrl+_",
   "desc": "todos os comandos"
  }
 },
 {
  "command": "app.quit",
  "keys": [
   "q",
   "ctrl+c"
  ],
  "mode": "normal",
  "help": {
   "key": "q/ctrl+c",
   "desc": "sai"
  }
 },
 {
  "command": "todo.dispatch",
  "keys": [
   "enter"
  ],
  "mode": "normal.todos",
  "help": {
   "key": "enter",
   "desc": "dispara um agent para a tarefa ou issue"
  }
 },
 {
  "command": "session.reply",
  "keys": [
   "r"
  ],
  "mode": "normal",
  "help": {
   "key": "r",
   "desc": "resposta rápida"
  }
 },
 {
  "command": "sessions.refresh",
  "keys": [
   "R"
  ],
  "mode": "normal",
  "help": {
   "key": "R",
   "desc": "atualiza e reordena"
  }
 },
 {
  "command": "inbox.summon",
  "keys": [
   "shift+space"
  ],
  "mode": "normal",
  "help": {
   "key": "shift+space",
   "desc": "inbox: vai para quem te espera"
  }
 }
];

export const DEMO_COMMANDS: CommandInfo[] = [
 {
  "name": "session.archive",
  "title": "arquiva/restaura (no contexto: todas)",
  "hint": "",
  "keys": [
   "X"
  ],
  "mode": "normal",
  "args": [
   {
    "name": "context",
    "type": "string",
    "prompt": "contexto",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": true
 },
 {
  "name": "sessions.archive_stale",
  "title": "arquivar todas as sessões paradas",
  "hint": "",
  "keys": null,
  "mode": "",
  "args": null,
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": true
 },
 {
  "name": "sessions.remove_ended",
  "title": "limpar chats encerrados (claude rm)",
  "hint": "",
  "keys": null,
  "mode": "",
  "args": null,
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": true
 },
 {
  "name": "session.stop",
  "title": "parar a sessão em background",
  "hint": "",
  "keys": [
   "x"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.remove",
  "title": "remover a sessão em background",
  "hint": "",
  "keys": [
   "dd"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.rename",
  "title": "renomear a sessão",
  "hint": "",
  "keys": [
   "n"
  ],
  "mode": "normal",
  "args": [
   {
    "name": "name",
    "type": "string",
    "prompt": "nome",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.move",
  "title": "mover a sessão para outro contexto",
  "hint": "",
  "keys": [
   "m"
  ],
  "mode": "normal",
  "args": [
   {
    "name": "context",
    "type": "string",
    "prompt": "contexto",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.new",
  "title": "novo agent em background",
  "hint": "bypass pede confirmação, salvo se for o permission_mode configurado",
  "keys": [
   "c"
  ],
  "mode": "normal",
  "args": [
   {
    "name": "dir",
    "type": "dir",
    "prompt": "diretório › ",
    "required": true
   },
   {
    "name": "prompt",
    "type": "string",
    "prompt": "prompt › ",
    "required": true
   },
   {
    "name": "perm",
    "type": "string",
    "prompt": "permission-mode › ",
    "required": false
   },
   {
    "name": "name",
    "type": "string",
    "prompt": "nome › ",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": true
 },
 {
  "name": "session.open",
  "title": "abre e foca",
  "hint": "",
  "keys": [
   "enter"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.preview",
  "title": "abre sem focar",
  "hint": "",
  "keys": [
   "o"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.fullscreen",
  "title": "tela cheia",
  "hint": "",
  "keys": [
   "O"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "pane.close",
  "title": "fecha painel",
  "hint": "",
  "keys": [
   "w"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "pane.zoom",
  "title": "zoom do painel",
  "hint": "",
  "keys": [
   "z"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "group.add",
  "title": "agrupa as sessões escolhidas",
  "hint": "grupos",
  "keys": null,
  "mode": "normal",
  "args": [
   {
    "name": "tag",
    "type": "tag",
    "prompt": "grupo",
    "required": true
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "group.remove",
  "title": "tira as sessões escolhidas do grupo",
  "hint": "grupos",
  "keys": null,
  "mode": "normal",
  "args": [
   {
    "name": "tag",
    "type": "tag",
    "prompt": "grupo",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "group.notify",
  "title": "avisa o grupo",
  "hint": "grupos",
  "keys": null,
  "mode": "normal",
  "args": [
   {
    "name": "tag",
    "type": "tag",
    "prompt": "grupo",
    "required": false
   },
   {
    "name": "text",
    "type": "string",
    "prompt": "mensagem",
    "required": true
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "group.dissolve",
  "title": "desfaz o grupo",
  "hint": "grupos",
  "keys": null,
  "mode": "normal",
  "args": [
   {
    "name": "tag",
    "type": "tag",
    "prompt": "grupo",
    "required": true
   }
  ],
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": true
 },
 {
  "name": "todo.start",
  "title": "começa a tarefa",
  "hint": "",
  "keys": [
   "s"
  ],
  "mode": "normal.todos",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "todo.done",
  "title": "marca a tarefa como done",
  "hint": "",
  "keys": [
   "D"
  ],
  "mode": "normal.todos",
  "args": null,
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": false
 },
 {
  "name": "jira.toggle",
  "title": "todo↔jira: issue vira tarefa, tarefa vira issue",
  "hint": "",
  "keys": [
   "J"
  ],
  "mode": "normal.todos",
  "args": [
   {
    "name": "target",
    "type": "string",
    "prompt": "projeto/tipo",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": false
 },
 {
  "name": "jira.fetch",
  "title": "busca as issues no Jira (via claude, ~$0.1)",
  "hint": "",
  "keys": [
   "F"
  ],
  "mode": "normal.todos",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "jira.open_url",
  "title": "abre a issue no browser",
  "hint": "",
  "keys": [
   "o"
  ],
  "mode": "normal.todos",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "nav.down",
  "title": "desce",
  "hint": "",
  "keys": [
   "j",
   "down"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.up",
  "title": "sobe",
  "hint": "",
  "keys": [
   "k",
   "up"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.top",
  "title": "topo",
  "hint": "",
  "keys": [
   "gg"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.bottom",
  "title": "fim",
  "hint": "",
  "keys": [
   "G"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.half_down",
  "title": "meia página ↓",
  "hint": "",
  "keys": [
   "ctrl+d"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.half_up",
  "title": "meia página ↑",
  "hint": "",
  "keys": [
   "ctrl+u"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.page_down",
  "title": "página ↓",
  "hint": "",
  "keys": [
   "ctrl+f",
   "pgdown"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.page_up",
  "title": "página ↑",
  "hint": "",
  "keys": [
   "ctrl+b",
   "pgup"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.collapse",
  "title": "fecha nó",
  "hint": "",
  "keys": [
   "h",
   "left"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.expand",
  "title": "abre nó",
  "hint": "",
  "keys": [
   "l",
   "right"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.pick",
  "title": "seleciona (p/ :group); no contexto: alterna",
  "hint": "",
  "keys": [
   "space"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.next_attention",
  "title": "próxima te esperando",
  "hint": "",
  "keys": [
   "a"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "nav.prev_attention",
  "title": "anterior te esperando",
  "hint": "",
  "keys": [
   "A"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "session.insert",
  "title": "INSERT no painel",
  "hint": "",
  "keys": [
   "i"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.sessions",
  "title": "sessões",
  "hint": "",
  "keys": [
   "1"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.inbox",
  "title": "inbox",
  "hint": "",
  "keys": [
   "2"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.todos",
  "title": "todos + jira",
  "hint": "",
  "keys": [
   "3"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.archive",
  "title": "arquivo",
  "hint": "",
  "keys": [
   "4"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.next",
  "title": "próxima aba",
  "hint": "",
  "keys": [
   "tab"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "tab.prev",
  "title": "aba anterior",
  "hint": "",
  "keys": [
   "shift+tab"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "view.filter",
  "title": "filtro fuzzy",
  "hint": "",
  "keys": [
   "/"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "view.show_all",
  "title": "mostra paradas/arquivadas/subagents concluídos",
  "hint": "",
  "keys": [
   "H"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "view.sort",
  "title": "ordem: atividade/custo/nome",
  "hint": "",
  "keys": [
   "S"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "view.theme",
  "title": "tema",
  "hint": "",
  "keys": [
   "T"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "switcher.open",
  "title": "ir p/ chat rodando/esperando",
  "hint": "",
  "keys": [
   "ctrl+k"
  ],
  "mode": "*",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "palette.open",
  "title": "comandos",
  "hint": "",
  "keys": [
   ":",
   "ctrl+p"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "usage.show",
  "title": "uso (/usage)",
  "hint": "",
  "keys": [
   "u"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "help.open",
  "title": "todos os comandos",
  "hint": "",
  "keys": [
   "ctrl+/",
   "?",
   "ctrl+_"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "app.quit",
  "title": "sai",
  "hint": "",
  "keys": [
   "q",
   "ctrl+c"
  ],
  "mode": "normal",
  "args": null,
  "scope": "client",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "todo.dispatch",
  "title": "dispara um agent para a tarefa ou issue",
  "hint": "marca start e anota o id do agent",
  "keys": [
   "enter"
  ],
  "mode": "normal.todos",
  "args": [
   {
    "name": "dir",
    "type": "dir",
    "prompt": "diretório › ",
    "required": false
   },
   {
    "name": "perm",
    "type": "string",
    "prompt": "permission-mode › ",
    "required": false
   }
  ],
  "scope": "core",
  "confirm": true,
  "source": "builtin",
  "available": false
 },
 {
  "name": "session.reply",
  "title": "resposta rápida",
  "hint": "um número sozinho escolhe a opção da pergunta",
  "keys": [
   "r"
  ],
  "mode": "normal",
  "args": [
   {
    "name": "text",
    "type": "string",
    "prompt": "resposta › ",
    "required": true
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": false
 },
 {
  "name": "sessions.refresh",
  "title": "atualiza e reordena",
  "hint": "",
  "keys": [
   "R"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "inbox.summon",
  "title": "inbox: vai para quem te espera",
  "hint": "a sessão mais urgente",
  "keys": [
   "shift+space"
  ],
  "mode": "normal",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "lua.reload",
  "title": "recarrega o init.lua e os plugins",
  "hint": "lua",
  "keys": null,
  "mode": "",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "lua.messages",
  "title": "últimos erros e mensagens lua",
  "hint": "lua",
  "keys": null,
  "mode": "",
  "args": null,
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 },
 {
  "name": "lua.enable",
  "title": "religa um arquivo lua desativado",
  "hint": "lua",
  "keys": null,
  "mode": "",
  "args": [
   {
    "name": "file",
    "type": "string",
    "prompt": "arquivo (ex.: plugins/x.lua)",
    "required": true
   }
  ],
  "scope": "core",
  "confirm": false,
  "source": "builtin",
  "available": true
 }
];
