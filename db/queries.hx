QUERY InsertDocument(title: String) =>
    d <- AddN<Document>({Title: title})
    RETURN d

QUERY GetDocumentByTitle(title: String) =>
    d <- N<Document>({Title: title})
    RETURN d

QUERY CountByTitle(title: String) =>
    cnt <- N<Document>({Title: title})::COUNT
    RETURN cnt

QUERY InsertRelated(from_title: String, to_title: String, kind: String) =>
    from <- N<Document>({Title: from_title})
    to <- N<Document>({Title: to_title})
    e <- AddE<Related>({Kind: kind})::From(from)::To(to)
    RETURN e

QUERY Neighbours(title: String) =>
    nbrs <- N<Document>({Title: title})::Out<Related>
    RETURN nbrs

QUERY InsertVector(title: String, vec: [F64]) =>
    v <- AddV<DocVec>(vec, {DocTitle: title})
    RETURN v

QUERY VectorSearch(vec: [F64], k: I32) =>
    hits <- SearchV<DocVec>(vec, k)
    RETURN hits
