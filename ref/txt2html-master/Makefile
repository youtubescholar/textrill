CC = gcc
CFLAGS = -pedantic -Wall -Wextra -Werror
SRC = $(wildcard *.c)
OBJ = $(SRC:.c=.o)
OUT = txt2html

all: $(OUT)

debug: clean
debug: CFLAGS += -g
debug: OUT = txt2html-debug
debug: $(OUT)


$(OUT): $(OBJ)
	$(CC) -o $(OUT) $(OBJ)

$(OBJ): $(SRC)
	$(CC) $(CFLAGS) -c $(SRC)


.PHONY: clean
clean:
	rm -f $(OBJ) $(OUT) $(OUT)-debug
