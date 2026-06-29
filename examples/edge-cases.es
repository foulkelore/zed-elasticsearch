# Edge cases for verifying .es highlighting.
# Open this in Zed and confirm each section highlights sensibly.

# 1) A request with NO body, at the very end of a line, followed by another
GET _cluster/health
GET _cat/nodes

# 2) Consecutive requests with bodies, no blank line between them
PUT index-a
{ "settings": { "number_of_shards": 1 } }
PUT index-b
{ "settings": { "number_of_shards": 2 } }

# 3) Empty object and empty array bodies
POST my-index/_search
{}

POST my-index/_search
{ "query": {}, "sort": [] }

# 4) Deeply nested body
POST my-index/_search
{
  "query": {
    "bool": {
      "must": [
        { "match": { "title": "a" } },
        { "nested": { "path": "comments", "query": { "match_all": {} } } }
      ]
    }
  }
}

# 5) Strings containing braces and quotes (must not break body detection)
POST my-index/_doc
{
  "title": "a string with { braces } and \"escaped quotes\"",
  "regex": "^[a-z]+$"
}

# 6) Numbers: negative, float, scientific
POST my-index/_doc
{ "a": -1, "b": 3.14, "c": 1.2e10, "d": 0 }

# 7) Query params with a body
POST my-index/_search?pretty&size=10&from=0
{ "query": { "match_all": {} } }

# 8) Comment styles back to back
# hash comment
// slash comment
GET _cluster/health

# 9) A trailing request with a body at end of file (no trailing newline issues)
DELETE my-index
{ "ignore_unavailable": true }
