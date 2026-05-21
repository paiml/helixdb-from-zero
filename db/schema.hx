N::Document {
    UNIQUE INDEX Title: String,
}

V::DocVec {
    DocTitle: String,
}

E::Related {
    From: Document,
    To: Document,
    Properties: {
        Kind: String,
    }
}
