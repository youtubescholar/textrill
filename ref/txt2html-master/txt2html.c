#include "txt2html.h"

struct node *convf(FILE *f);
int readn(struct node *n);

static uint8_t opts; // make extern if passing it about becomes a pain

void help()
{
	puts("usage: txt2html [OPTIONS] FILE...");
	puts("");
	puts("Convert content in txt files to html.");
	puts("");
	puts("FILE...   A list of filepaths that point to files to be converted to HTML");
	puts("");
	puts("OPTIONS");
	puts("-br           Treat newlines within paragraphs as line breaks.");
	puts("-v            Print verbose logs during runtime");
	puts("-h, --help    Print this message");
}

void verbose(const char *fmt, ...)
{
	if (opts & OPT_V) {
		printf("txt2html: ");
		va_list args;
		va_start(args, fmt);
		vprintf(fmt, args);
		va_end(args);
		fflush(stdout);
	}
}

void parseargs(int argc, char **argv)
{
	int i = 0, a = argc-1;
	for (; a > 0; --a) {
		if (argv[a] == NULL)
			continue;
		if (argv[a][i] == '-') {
			if (strcmp(argv[a], "-h") == 0 ||
				strcmp(argv[a], "--help") == 0) {
				help();
				exit(0);
			} else if (strcmp(argv[a], "-br") == 0) {
				opts |= OPT_BR;
			} else if (strcmp(argv[a], "-v") == 0) {
				opts |= OPT_V;
			} else if (strcmp(argv[a], "-nm") == 0) {
				opts |= OPT_NM;
			}
			
			argv[a][0] = '\0';
		}
	}
}

int main(int argc, char **argv)
{
	parseargs(argc, argv);
	verbose("printing verbose logs\n");

	int a;
	FILE *f;
	struct node *n;
	for (a = 1; a < argc; ++a) {
		if (strlen(argv[a]) == 0)
			continue;

		verbose("opening %s\n", argv[a]);
		if ((f = fopen(argv[a], "r")) == NULL) {
			perror("fopen failed, abort");
			continue;
		}

		n = convf(f);
		verbose("counted %d nodes\n", readn(n));
		verbose("closing %s\n", argv[a]);
		if (fclose(f) == EOF) perror("fclose failed");

		while (!n) {
			if (n->buf && n->buf[strlen(n->buf)+1] == '$')
				free(n->buf);
			if (n->next) free(n->next);
			if (n->prev) {
				n = n->prev;
			} else {
				free(n);
				break;
			}
		}
		node_create(NULL, 0); // reset node count
	}

	return EXIT_SUCCESS;
}

struct node *convf(FILE *f)
{
	int siz;
	struct node *n = 0;
	char buf[BUFSIZ] = {'\0'};
	while (true) {
		siz = fread(buf, sizeof(char), BUFSIZ-1, f);
		if (siz == 0) break;
		buf[siz+1] = '\0';
		verbose("read %d bytes\n", siz);
		n = parse_buf(buf, &n, opts);
	}
	n = parse_buf(NULL, &n, opts);
	return n;
}

int readn(struct node *n)
{
	while (n->prev)
		n = n->prev; // rewind
	int cnt = 0;
	while (n) {
		if (n->buf) printf("%s", n->buf);
		n = n->next;
		++cnt;
	}
	return cnt;
}
