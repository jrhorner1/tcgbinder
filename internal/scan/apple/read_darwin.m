// Reading a card with Vision: find the card's outline, straighten it, read the text.

#import <Foundation/Foundation.h>
#import <CoreImage/CoreImage.h>
#import <ImageIO/ImageIO.h>
#import <Vision/Vision.h>
#include <stdlib.h>
#include <string.h>
#import "read.h"

// A card is 63 × 88 mm; Vision's aspect ratio is short side over long side.
static const float kCardAspect = 63.0f / 88.0f;

// tcgbStraighten finds the card and returns it cropped, straightened, upright-ish and
// large enough for the small collector text, or nil if no card outline was found.
static CIImage *tcgbStraighten(CIImage *image) {
	VNDetectRectanglesRequest *rect = [[VNDetectRectanglesRequest alloc] init];
	rect.minimumAspectRatio = kCardAspect - 0.12f;
	rect.maximumAspectRatio = kCardAspect + 0.12f;
	rect.minimumSize = 0.2f;
	rect.maximumObservations = 1;
	rect.quadratureTolerance = 25.0f;
	rect.minimumConfidence = 0.5f;
	VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCIImage:image options:@{}];
	if (![handler performRequests:@[ rect ] error:nil]) {
		return nil;
	}
	VNRectangleObservation *obs = (VNRectangleObservation *)rect.results.firstObject;
	if (obs == nil) {
		return nil;
	}
	CGRect ext = image.extent;
	CIVector * (^corner)(CGPoint) = ^CIVector *(CGPoint p) {
		return [CIVector vectorWithX:ext.origin.x + p.x * ext.size.width Y:ext.origin.y + p.y * ext.size.height];
	};
	CIFilter *fix = [CIFilter filterWithName:@"CIPerspectiveCorrection"];
	[fix setValue:image forKey:kCIInputImageKey];
	[fix setValue:corner(obs.topLeft) forKey:@"inputTopLeft"];
	[fix setValue:corner(obs.topRight) forKey:@"inputTopRight"];
	[fix setValue:corner(obs.bottomLeft) forKey:@"inputBottomLeft"];
	[fix setValue:corner(obs.bottomRight) forKey:@"inputBottomRight"];
	CIImage *card = fix.outputImage;
	if (card == nil) {
		return nil;
	}
	if (card.extent.size.width > card.extent.size.height) { // lying sideways: stand it up
		card = [card imageByApplyingCGOrientation:kCGImagePropertyOrientationRight];
	}
	CGFloat scale = 1600.0 / card.extent.size.height; // collector text is ~1.5% of the height
	if (scale > 1.0) {
		CIFilter *up = [CIFilter filterWithName:@"CILanczosScaleTransform"];
		[up setValue:card forKey:kCIInputImageKey];
		[up setValue:@(scale) forKey:kCIInputScaleKey];
		[up setValue:@1.0 forKey:kCIInputAspectRatioKey];
		card = up.outputImage;
	}
	return [card imageByApplyingTransform:CGAffineTransformMakeTranslation(-card.extent.origin.x, -card.extent.origin.y)];
}

// tcgbRecognize reads the text in the image. score is the number of confidently read
// letters, used to choose between orientations.
static NSArray *tcgbRecognize(CIImage *image, double *score) {
	VNRecognizeTextRequest *req = [[VNRecognizeTextRequest alloc] init];
	req.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
	req.usesLanguageCorrection = NO; // card names are made-up words
	req.minimumTextHeight = 0.008f;
	VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCIImage:image options:@{}];
	NSMutableArray *lines = [NSMutableArray array];
	*score = 0;
	if (![handler performRequests:@[ req ] error:nil]) {
		return lines;
	}
	NSCharacterSet *letters = [NSCharacterSet letterCharacterSet];
	for (VNRecognizedTextObservation *obs in req.results) {
		VNRecognizedText *text = [obs topCandidates:1].firstObject;
		if (text == nil || text.string.length == 0) {
			continue;
		}
		CGRect box = obs.boundingBox; // normalized, origin at the bottom-left
		[lines addObject:@{
			@"text" : text.string,
			@"x" : @(box.origin.x),
			@"y" : @(1.0 - box.origin.y - box.size.height),
			@"w" : @(box.size.width),
			@"h" : @(box.size.height),
			@"confidence" : @(text.confidence),
		}];
		NSUInteger n = 0;
		for (NSUInteger i = 0; i < text.string.length; i++) {
			if ([letters characterIsMember:[text.string characterAtIndex:i]]) {
				n++;
			}
		}
		*score += n * text.confidence;
	}
	return lines;
}

NSDictionary *TCGBReadCard(CIImage *image) {
	@autoreleasepool {
		CIImage *card = tcgbStraighten(image);
		CIImage *source = card ? card : image;
		double score = 0;
		NSArray *lines = tcgbRecognize(source, &score);
		// Upside down (or turned the other way): little reads confidently, so try the
		// other way up and keep the better reading.
		if (score < 40) {
			double flipped = 0;
			CIImage *turned = [source imageByApplyingCGOrientation:kCGImagePropertyOrientationDown];
			turned = [turned imageByApplyingTransform:CGAffineTransformMakeTranslation(-turned.extent.origin.x, -turned.extent.origin.y)];
			NSArray *other = tcgbRecognize(turned, &flipped);
			if (flipped > score * 1.5) {
				lines = other;
			}
		}
		return @{ @"lines" : lines, @"card_found" : (card != nil ? @YES : @NO) };
	}
}

char *TCGBJSONString(NSDictionary *dict) {
	NSData *data = [NSJSONSerialization dataWithJSONObject:dict options:0 error:nil];
	if (data == nil) {
		return NULL;
	}
	char *out = malloc(data.length + 1);
	memcpy(out, data.bytes, data.length);
	out[data.length] = 0;
	return out;
}

char *tcgb_read_image(const void *bytes, int length, char **err) {
	@autoreleasepool {
		NSData *data = [NSData dataWithBytes:bytes length:length];
		// Photos carry their orientation in EXIF; apply it so the card is upright.
		CIImage *image = [CIImage imageWithData:data options:@{kCIImageApplyOrientationProperty : @YES}];
		if (image == nil) {
			*err = strdup("the file isn't an image that can be read");
			return NULL;
		}
		image = [image imageByApplyingTransform:CGAffineTransformMakeTranslation(-image.extent.origin.x, -image.extent.origin.y)];
		return TCGBJSONString(TCGBReadCard(image));
	}
}
