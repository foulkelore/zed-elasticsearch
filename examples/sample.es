# Sample Elasticsearch Console (.es) file
# Used for manually testing the extension's syntax highlighting.

# --- Simple requests (no body) ---

GET _cluster/health

GET _cat/indices?v

// You can also use double-slash comments

DELETE my-index

# --- Requests with a JSON body ---

PUT my-index
{
  "settings": {
    "number_of_shards": 1,
    "number_of_replicas": 0
  },
  "mappings": {
    "properties": {
      "title": { "type": "text" },
      "views": { "type": "integer" },
      "published": { "type": "boolean" }
    }
  }
}

POST my-index/_doc
{
  "title": "Getting started with Elasticsearch",
  "views": 42,
  "published": true,
  "tags": ["search", "elasticsearch", "tutorial"]
}

# --- Search with query params and a body ---

POST my-index/_search?pretty&size=10
{
  "query": {
    "bool": {
      "must": [
        { "match": { "title": "elasticsearch" } }
      ],
      "filter": [
        { "range": { "views": { "gte": 10 } } }
      ]
    }
  }
}

# --- Bulk request (newline-delimited JSON) ---

POST _bulk
{ "index": { "_index": "my-index", "_id": "1" } }
{ "title": "First", "views": 1 }
{ "index": { "_index": "my-index", "_id": "2" } }
{ "title": "Second", "views": 2 }
