#ifndef _TXT2HTML_H_
#define _TXT2HTML_H_

#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <stdint.h>
#include <string.h>
#include <stdbool.h>
#include <ctype.h> // TODO replace with utf8 support
#include <assert.h>

#define OPT_V  0x10 // print verbose logs
#define OPT_NM 0x20 // no memory limit
#define OPT_BR 0x01 // newlines as <br/> nodes within <p> (not ' ')

typedef uint8_t NodeType;

enum {
	NONE  = 0x00,
	OPEN  = 0x10,
	CLOSE = 0x20,
	
	H1  = 0x01,
	H2  = 0x02,
	P   = 0x03,
	PRE = 0x04,
	BR  = 0x05,
	LI  = 0x06,
	OL  = 0x07,
	UL  = 0x08
};

struct node {
	struct node *prev, *next;
	NodeType type;
	char *buf;
};

/*--------
  rule.c
--------*/
// get the length of a rule for `NodeType t`.
size_t rule_len(NodeType t);

// check if `str` matches the rule for `NodeType t`.
bool rule_match(const char *str, NodeType t);

// return H1 or H2 if `str` matches said NodeType.
// If it matches neither 0 will be returned.y
NodeType rule_match_heading(const char *str);

/*--------
  node.c
--------*/
struct node *node_create(struct node *prev, NodeType t);

// write a character to `n->buf`.
// Has an internal static buffer (`buf`) that `c` is written to.
// If `c == EOF` or `buf` reaches `BUFSIZ` or `n` does not match
// `n` from the previous call, then `buf` is written to the previous
// `n` and reset for a new set of data.
void node_writec(struct node **n, int c);

// rule `str` against a set of rules and determine the next node type.
// `n` will be updated to a newly created node of the determined type.
size_t node_next(const char *str, struct node **n);

/*---------
  parse.c
---------*/
// main parsing function
struct node *parse_buf(const char *buf, struct node **n, uint8_t opts);

// parse `str` into `n` until *\0* or *\n\n* is found.
// If `opts & OPT_BR` then `\n` will be parsed as a `<br/>` node.
// If `n->type` is *PRE*, then parsing will also stop after the first
// `\n` that is not followed by a `\t`.
// The number of parsed bytes is returned
size_t parse_textblock(const char *str, struct node **n, bool softbreaks);

// parse a line of text from `str` into `n` and skip the line after
// aslong as it contains *=* or *-*.
// The number of parsed bytes is returned.
size_t parse_heading(const char *str, struct node **n);

// parse `str` into `n` for *OL+LI* until *CLOSE+OL*.
// The number of parsed bytes is returned.
size_t parse_oli(const char *str, struct node **n, uint8_t opts);

// parse `str` into`n` until *\0* or *\n\n*. After this, assign
// a new node to `n` of CLOSE+P.
// The number of parsed bytes is returned.
size_t parse_p(const char *str, struct node **n, uint8_t opts);

// parse `str` into `n` for *UL+LI* until *CLOSE+UL*.
// The number of parsed bytes is returned.
size_t parse_uli(const char *str, struct node **n, uint8_t opts);

#endif
