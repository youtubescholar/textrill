#include "txt2html.h"

struct node *parse_buf(const char *buf, struct node **n, uint8_t opts)
{
	size_t i = 0;
	size_t len = (buf) ? strlen(buf) : 0;

	if (!buf && (*n)) {
		*n = node_create(*n, CLOSE+(*n)->type);
		i = len;
	}

	while (i < len && buf) {
		while (buf[i] == '\n') ++i;
		if (!(*n) || ((*n)->type & CLOSE))
			i += node_next(&buf[i], n);
		switch ((*n)->type) {
			case H1:
			case H2:
				i += parse_heading(&buf[i], n);
				break;
			case P:     i += parse_p(&buf[i], n, opts);   break;
			case OL+LI: i += parse_oli(&buf[i], n, opts); break;
			case UL+LI: i += parse_uli(&buf[i], n, opts); break;
			case PRE:
				i += parse_textblock(&buf[i], n, opts & OPT_BR);
				*n = node_create(*n, CLOSE+PRE);
				break;
			default:
				i += node_next(&buf[i], n);
				break;
		}
	}
	
	return *n;
}

size_t parse_textblock(const char *str, struct node **n, bool softbreaks)
{
	size_t ret = 0;

	while (str[ret] != '\0' && (isprint(str[ret]) || str[ret] == '\n' || str[ret] == '\t')) {
		if (str[ret] == '\n' && str[ret+1] == '\n')
			break;
		else if ((*n)->type == PRE && str[ret] == '\t')
			++ret;
		else if (str[ret] == '\n') {
			if (((*n)->type & (OL+LI) && rule_match(&str[ret+1], OPEN+OL+LI)) ||
				((*n)->type & (UL+LI) && rule_match(&str[ret+1], OPEN+UL+LI))) {
				++ret;
				break;
			}

			if ((*n)->type == PRE && str[ret+1] == '\t') {
				node_writec(n, '\n');
				++ret;
			} else if ((*n)->type == PRE && str[ret+1] != '\t') {
				break;
			} else if (softbreaks) {
				*n = node_create(*n, OPEN+BR+CLOSE);
				*n = node_create(*n, (*n)->prev->type);
			} else {
				node_writec(n, str[ret]);
			}
		} else {
			node_writec(n, str[ret]);
		}
		++ret;
	}
	node_writec(n, EOF);

	return ret;
}

size_t parse_heading(const char *str, struct node **n)
{
	assert(str);
	size_t i = 0;
	while(str[i] && str[i] != '\n')
		node_writec(n, str[i++]);
	node_writec(n, EOF);
	do { ++i; } while (str[i] == '-' || str[i] == '=');
	*n = node_create(*n, CLOSE+(*n)->type);
	return i;
}

size_t parse_oli(const char *str, struct node **n, uint8_t opts)
{
	assert(str);
	size_t i = 0, len = strlen(str);
	while(i < len) {
		i += parse_textblock(&str[i], n, opts & OPT_BR);
		*n = node_create(*n, CLOSE+OL+LI);

		if (str[i] == '\0' || rule_match(&str[i], CLOSE+OL)) {
			i += rule_len(CLOSE+OL);
			*n = node_create(*n, CLOSE+OL);
			break;
		} else if (rule_match(&str[i], OPEN+OL+LI)) {
			i += rule_len(OPEN+OL+LI);
			*n = node_create(*n, OPEN+OL+LI);
			*n = node_create(*n, OL+LI);
		}
	}
	return i;
}

size_t parse_p(const char *str, struct node **n, uint8_t opts)
{
	size_t i = parse_textblock(str, n, opts & OPT_BR);
	if (str[i] == '\n' && str[i+1] == '\n')
		*n = node_create(*n, CLOSE+P);
	return i;
}

size_t parse_uli(const char *str, struct node **n, uint8_t opts)
{
	size_t ret = 0;
	size_t len = strlen(str);

	while (ret < len) {
		ret += parse_textblock(&str[ret], n, opts & OPT_BR);
		*n = node_create(*n, CLOSE+UL+LI);

		if (str[ret] == '\0' || rule_match(&str[ret], CLOSE+UL)) {
			ret += rule_len(CLOSE+UL);
			*n = node_create(*n, CLOSE+UL);
			break;
		} else if (rule_match(&str[ret], OPEN+UL+LI)) {
			ret += rule_len(OPEN+UL+LI);
			*n = node_create(*n, OPEN+UL+LI);
			*n = node_create(*n, UL+LI);
		} else break;
	}

	return ret;
}
