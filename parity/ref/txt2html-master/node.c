#include "txt2html.h"

struct node *node_create(struct node *prev, NodeType t)
{
	struct node *n = calloc(1, sizeof(struct node));

	if (prev) {
		n->prev = prev;
		prev->next = n;
	}

	n->type = t;

	if (t == OPEN+BR+CLOSE) {
		n->buf = "<br/>\n\0";
	} else if (t & OPEN) {
		t &= 0x0F;
		if      (t == H1)  n->buf = "<h1>\0";
		else if (t == H2)  n->buf = "<h2>\0";
		else if (t == PRE) n->buf = "<pre>\0";
		else if (t == P)   n->buf = "<p>\0";
		else if (t == OL)  n->buf = "<ol>\n\0";
		else if (t == UL)  n->buf = "<ul>\n\0";
		else if (t == OL+LI || t == UL+LI)
			n->buf = "<li>\0";
	} else if (t & CLOSE) {
		t &= 0x0F;
		node_writec(&prev, EOF);
		if      (t == H1)  n->buf = "</h1>\n\0";
		else if (t == H2)  n->buf = "</h2>\n\0";
		else if (t == PRE) n->buf = "</pre>\n\0";
		else if (t == P)   n->buf = "</p>\n\0";
		else if (t == OL)  n->buf = "</ol>\n\0";
		else if (t == UL)  n->buf = "</ul>\n\0";
		else if (t == UL+LI || t == OL+LI)
			n->buf = "</li>\n\0";
	}

	return n;
}

// writebuf has an internal static buffer (`buf`) that it writes `c` to.
// if `c == EOF` or `buf` reaches `BUFSIZ`, then `buf` it's written to n->buf.
// `n->buf` will only be allocated required memory.
void node_writec(struct node **n, int c)
{
	assert(n);

	static int pg = 0;
	static int len = 0;
	static char buf[BUFSIZ+1];

	if (len+2 == BUFSIZ || (c == EOF && len > 0)) {
		if (c == EOF) {
			buf[len++] = '\0';
			buf[len++] = '$'; // signal malloc not assigned const
		}
		(*n)->buf = (pg == 0) ? malloc(len) : realloc((*n)->buf, strlen((*n)->buf) + len);
		memmove((*n)->buf, buf, len);
		++pg;
		len = 0;
		memset(buf, '\0', BUFSIZ); 
	}

	switch (c) {
		case EOF:
			pg = 0;
			break;
		case '\t':
			strncat(buf, "&emsp;", 7);
			len += 6;
			break;
		default:
			strncat(buf, (char *)&c, 2);
			len += 1;
			break;
	}
}

size_t node_next(const char *str, struct node **n)
{
	size_t ret = 0;
	if (rule_match(&str[ret], OPEN+OL+LI)) {
		ret += rule_len(OPEN+OL+LI);
		*n = node_create(*n, OPEN+OL);
		*n = node_create(*n, OPEN+OL+LI);
		*n = node_create(*n, OL+LI);
	} else if (rule_match(&str[ret], OPEN+UL+LI)) {
		ret += rule_len(OPEN+UL+LI);
		*n = node_create(*n, OPEN+UL);
		*n = node_create(*n, OPEN+UL+LI);
		*n = node_create(*n, UL+LI);
	} else if (rule_match(&str[ret], OPEN+PRE)) {
		ret += rule_len(OPEN+PRE);
		*n = node_create(*n, OPEN+PRE);
		*n = node_create(*n, PRE);
	} else if (isprint(str[ret])) {
		switch (rule_match_heading(&str[ret])) {
			case H1:
				*n = node_create(*n, OPEN+H1);
				*n = node_create(*n, H1);
				break;
			case H2:
				*n = node_create(*n, OPEN+H2);
				*n = node_create(*n, H2);
				break;
			default:
				*n = node_create(*n, OPEN+P);
				*n = node_create(*n, P);
				break;
		}
	}
	return ret;
}
