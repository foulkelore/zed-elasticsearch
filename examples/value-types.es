# Value-type color check for .es body highlighting.
#
# Open this in Zed and confirm each value type below has a distinct,
# sensible color. The KEY (left of the colon) and the VALUE (right of the
# colon) should look different from each other, and booleans/null should not
# look like plain unstyled text.

POST color-check/_doc
{
  "a_string": "strings look like this",
  "a_number": 42,
  "a_negative": -7,
  "a_float": 3.14159,
  "scientific": 1.2e10,
  "is_true": true,
  "is_false": false,
  "nothing": null,
  "an_array": ["one", 2, true, null],
  "nested_object": {
    "inner_key": "inner value",
    "inner_number": 100
  }
}

# Booleans and null on their own lines (easiest to spot a missing color):
POST color-check/_doc
{
  "enabled": true,
  "disabled": false,
  "deleted_at": null
}
