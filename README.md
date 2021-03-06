# Mapillary Tools Extended

Some tools I use to process Mapillary images.

## Config

Create a file called `config.yml` with the following structure:

```yaml
# The directory to read images from
input_directory: data
# The directory to put images in that could not be processed
failed_directory: failed
# Areas that you want to exclude from Mapillary. Any image in this zone will be moved to the failed_directory
privacy_zones:
  - name: Home
    centre:
      latitude: 51.12025812870583
      longitude: -1.3962235945372675
    # Distance in metres
    distance: 45
  - name: Parents' House
    centre:
      latitude: 51.2841183328633
      longitude: -1.1910003863281807
    distance: 50
```
