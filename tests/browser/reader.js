// The member the first launch adds after the owner: a reader, whose own
// session shows what a colleague sees. Throwaway values for a throwaway
// instance on 127.0.0.1.
import { join } from "node:path";

export const READER = {
    name: "Reader",
    email: "reader@example.com",
    password: "Browser_reader_12",
};

export const READER_SESSION = join(import.meta.dirname, ".auth", "reader.json");
