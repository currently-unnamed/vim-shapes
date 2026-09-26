# Ontology

## Object types

### Airport

- API name: `airport`
- Primary key: `code`
- Title: `name`

| property | type | key | title | shared | value type |
| --- | --- | --- | --- | --- | --- |
| `code` | string | ⚿ |  |  |  |
| `name` | string |  | ✎ |  | Name |
| `location` | geopoint |  |  |  |  |
| `email` | string |  |  | ✱ |  |

Link types:

- **departures** — one Airport to many Flight (`departures`)

Implements: Place

### Flight *(experimental)*

- API name: `flight`
- Primary key: `id`
- Title: `departs`
- Backed by: flights.parquet

| property | type | key | title | shared | value type |
| --- | --- | --- | --- | --- | --- |
| `id` | string | ⚿ |  |  |  |
| `departs` | timestamp |  | ✎ |  |  |
| `delayed` | boolean |  |  |  |  |

Link types:

- **departures** — many Flight to one Airport (`origin`)

Actions: Delay flight modifies

## Interfaces

### Place

- API name: `place`
- Implemented by: Airport

| property | type | shared | value type |
| --- | --- | --- | --- |
| `location` | geopoint |  |  |

## Action types

### Delay flight

- API name: `delayFlight`

| parameter | type | required |
| --- | --- | --- |
| `flight` | object reference |  |
| `minutes` | integer | yes |

Rules: modifies Flight

## Datasources

- **flights.parquet**

