import time
import pyperclip

# split file into lines in chunk_size character chunks
def split_chinked_lines(file, chunk_size=(2000 - 11)) -> None:
    with open(file) as f:
        chunks = []
        prepend = ""
        while True:
            chunk = prepend + f.read(chunk_size - len(prepend))
            if not chunk:
                break
            pyperclip.copy(f"```ansi {chunk}```")
            print("copied chunk to clipboard")
            time.sleep(3)
            prepend = chunk[chunk.rfind('\n') + 1:]

if __name__ == "__main__":
    split_chinked_lines("filesystem-test.ansi")

