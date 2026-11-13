// The Vision card reader, called from Rust (src/lib.rs).
#import <Foundation/Foundation.h>
#import <CoreImage/CoreImage.h>

// TCGBReadCard finds the card in the image, straightens it and reads its text. The
// result is a scan.Reading as a JSON-ready dictionary.
NSDictionary *TCGBReadCard(CIImage *image);

// TCGBJSONString encodes a dictionary as a malloc'd UTF-8 C string (free with free()).
char *TCGBJSONString(NSDictionary *dict);

// tcgb_read_image reads a photo's bytes (JPEG, PNG, HEIC, ...). It returns the reading
// as JSON, or NULL with *err set; both are malloc'd.
char *tcgb_read_image(const void *data, int length, char **err);
