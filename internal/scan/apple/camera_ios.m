// The iOS card scanner: a full-screen camera view that reads each frame with Vision
// and hands the readings to Go, which says when it has a card. The photo library is
// offered too (and used on devices without a camera, like the Simulator).

#import <UIKit/UIKit.h>
#import <AVFoundation/AVFoundation.h>
#import <PhotosUI/PhotosUI.h>
#import <CoreImage/CoreImage.h>
#include <stdlib.h>
#include <string.h>
#import "read.h"
#include "_cgo_export.h"

// Go callbacks (camera_ios.go).
//   tcgbScanFrame: 0 nothing, 1 partial (hold still), 2 accept this reading.
//   tcgbScanDone: the final reading (JSON) or an error; cancelled when closed.

static UIViewController *tcgbTopController(void) {
	UIViewController *vc = [[[[UIApplication sharedApplication] delegate] window] rootViewController];
	while (vc.presentedViewController != nil) {
		vc = vc.presentedViewController;
	}
	return vc;
}

@interface TCGBScanController : UIViewController <AVCaptureVideoDataOutputSampleBufferDelegate, PHPickerViewControllerDelegate>
@property(nonatomic) uintptr_t handle;
@property(nonatomic) BOOL photosOnly;
@property(nonatomic, strong) AVCaptureSession *session;
@property(nonatomic, strong) AVCaptureDevice *device;
@property(nonatomic, strong) AVCaptureVideoPreviewLayer *preview;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic, strong) UILabel *status;
@property(nonatomic, strong) UIView *guide;
@property(atomic) BOOL busy;
@property(atomic) BOOL finished;
@property(atomic) BOOL captureNow;
@property(atomic) CFTimeInterval lastFrame;
@end

@implementation TCGBScanController

- (void)viewDidLoad {
	[super viewDidLoad];
	self.view.backgroundColor = UIColor.blackColor;
	self.queue = dispatch_queue_create("io.github.jrhorner1.tcgbinder.scan", DISPATCH_QUEUE_SERIAL);

	// Card-shaped guide in the middle of the screen.
	self.guide = [[UIView alloc] init];
	self.guide.layer.borderColor = [UIColor colorWithWhite:1 alpha:0.85].CGColor;
	self.guide.layer.borderWidth = 3;
	self.guide.layer.cornerRadius = 16;
	self.guide.userInteractionEnabled = NO;
	[self.view addSubview:self.guide];

	self.status = [[UILabel alloc] init];
	self.status.textColor = UIColor.whiteColor;
	self.status.textAlignment = NSTextAlignmentCenter;
	self.status.numberOfLines = 2;
	self.status.font = [UIFont systemFontOfSize:17 weight:UIFontWeightSemibold];
	self.status.backgroundColor = [UIColor colorWithWhite:0 alpha:0.55];
	self.status.layer.cornerRadius = 10;
	self.status.clipsToBounds = YES;
	self.status.text = @"Place a card inside the frame";
	[self.view addSubview:self.status];

	[self addButton:@"Cancel" action:@selector(cancel) tag:1];
	[self addButton:@"Photos" action:@selector(choosePhoto) tag:2];
	[self addButton:@"Capture" action:@selector(capture) tag:3];
	[self addButton:@"Light" action:@selector(toggleTorch) tag:4];
}

- (void)addButton:(NSString *)title action:(SEL)action tag:(NSInteger)tag {
	UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
	[b setTitle:title forState:UIControlStateNormal];
	[b setTitleColor:UIColor.whiteColor forState:UIControlStateNormal];
	b.titleLabel.font = [UIFont systemFontOfSize:18 weight:(tag == 3 ? UIFontWeightBold : UIFontWeightMedium)];
	b.backgroundColor = tag == 3 ? [UIColor colorWithRed:0.15 green:0.45 blue:0.95 alpha:0.95] : [UIColor colorWithWhite:0 alpha:0.55];
	b.layer.cornerRadius = 22;
	b.tag = tag;
	[b addTarget:self action:action forControlEvents:UIControlEventTouchUpInside];
	[self.view addSubview:b];
}

- (void)viewDidLayoutSubviews {
	[super viewDidLayoutSubviews];
	CGRect bounds = self.view.bounds;
	UIEdgeInsets safe = self.view.safeAreaInsets;
	self.preview.frame = bounds;
	CGFloat w = MIN(bounds.size.width * 0.82, (bounds.size.height - safe.top - safe.bottom - 200) * 63.0 / 88.0);
	CGFloat h = w * 88.0 / 63.0;
	self.guide.frame = CGRectMake((bounds.size.width - w) / 2, safe.top + 70 + (bounds.size.height - safe.top - safe.bottom - 200 - h) / 2, w, h);
	self.status.frame = CGRectMake(20, CGRectGetMaxY(self.guide.frame) + 12, bounds.size.width - 40, 52);
	CGFloat bottom = bounds.size.height - safe.bottom - 64;
	for (UIView *v in self.view.subviews) {
		if (![v isKindOfClass:UIButton.class]) {
			continue;
		}
		switch (v.tag) {
		case 1: v.frame = CGRectMake(16, safe.top + 12, 100, 44); break;
		case 4: v.frame = CGRectMake(bounds.size.width - 116, safe.top + 12, 100, 44); break;
		case 2: v.frame = CGRectMake(16, bottom, 110, 48); break;
		case 3: v.frame = CGRectMake(bounds.size.width - 156, bottom, 140, 48); break;
		}
	}
}

- (void)viewDidAppear:(BOOL)animated {
	[super viewDidAppear:animated];
	if (self.photosOnly) {
		self.photosOnly = NO;
		[self choosePhoto];
		return;
	}
	if (self.session == nil) {
		[self startCamera];
	}
}

- (void)startCamera {
	AVAuthorizationStatus auth = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
	if (auth == AVAuthorizationStatusNotDetermined) {
		[AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL granted) {
			dispatch_async(dispatch_get_main_queue(), ^{
				if (granted) {
					[self startCamera];
				} else {
					[self finishWithReading:nil error:@"Camera access was declined. Allow it in Settings → TCG Binder → Camera, or choose a photo."];
				}
			});
		}];
		return;
	}
	if (auth != AVAuthorizationStatusAuthorized) {
		self.status.text = @"Camera access is off. Allow it in Settings, or choose a photo.";
		return;
	}
	self.device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
	NSError *err = nil;
	AVCaptureDeviceInput *input = self.device ? [AVCaptureDeviceInput deviceInputWithDevice:self.device error:&err] : nil;
	if (input == nil) {
		self.status.text = @"No camera available. Choose a photo instead.";
		return;
	}
	if ([self.device lockForConfiguration:nil]) { // cards are close: focus near, continuously
		if ([self.device isFocusModeSupported:AVCaptureFocusModeContinuousAutoFocus]) {
			self.device.focusMode = AVCaptureFocusModeContinuousAutoFocus;
		}
		if (self.device.isAutoFocusRangeRestrictionSupported) {
			self.device.autoFocusRangeRestriction = AVCaptureAutoFocusRangeRestrictionNear;
		}
		[self.device unlockForConfiguration];
	}
	AVCaptureSession *session = [[AVCaptureSession alloc] init];
	session.sessionPreset = AVCaptureSessionPreset1920x1080; // small collector text needs pixels
	[session addInput:input];
	AVCaptureVideoDataOutput *output = [[AVCaptureVideoDataOutput alloc] init];
	output.alwaysDiscardsLateVideoFrames = YES;
	output.videoSettings = @{(id)kCVPixelBufferPixelFormatTypeKey : @(kCVPixelFormatType_32BGRA)};
	[output setSampleBufferDelegate:self queue:self.queue];
	[session addOutput:output];
	self.session = session;
	self.preview = [AVCaptureVideoPreviewLayer layerWithSession:session];
	self.preview.videoGravity = AVLayerVideoGravityResizeAspectFill;
	self.preview.frame = self.view.bounds;
	[self.view.layer insertSublayer:self.preview atIndex:0];
	dispatch_async(self.queue, ^{
		[session startRunning];
	});
}

// Each frame (at most ~4 a second, one at a time): read it and ask Go about it.
- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)buffer fromConnection:(AVCaptureConnection *)connection {
	CFTimeInterval now = CACurrentMediaTime();
	if (self.finished || self.busy || now - self.lastFrame < 0.25) {
		return;
	}
	self.busy = YES;
	self.lastFrame = now;
	CVPixelBufferRef pixels = CMSampleBufferGetImageBuffer(buffer);
	// The back camera's buffers are landscape; in portrait the picture is turned right.
	CIImage *image = [[CIImage imageWithCVPixelBuffer:pixels] imageByApplyingCGOrientation:kCGImagePropertyOrientationRight];
	image = [image imageByApplyingTransform:CGAffineTransformMakeTranslation(-image.extent.origin.x, -image.extent.origin.y)];
	NSDictionary *reading = TCGBReadCard(image);
	BOOL force = self.captureNow && [reading[@"lines"] count] > 0;
	char *json = TCGBJSONString(reading);
	int verdict = json ? tcgbScanFrame(self.handle, json) : 0;
	free(json);
	self.busy = NO;
	dispatch_async(dispatch_get_main_queue(), ^{
		if (self.finished) {
			return;
		}
		if (verdict == 2 || force) {
			[self finishWithReading:reading error:nil];
		} else if (verdict == 1) {
			self.status.text = @"Hold still…";
			self.guide.layer.borderColor = [UIColor colorWithRed:0.3 green:0.85 blue:0.4 alpha:1].CGColor;
		} else {
			self.status.text = self.captureNow ? @"No text found. Move closer, or try more light." : @"Place a card inside the frame";
			self.guide.layer.borderColor = [UIColor colorWithWhite:1 alpha:0.85].CGColor;
			self.captureNow = NO;
		}
	});
}

- (void)capture {
	self.captureNow = YES;
	self.status.text = @"Reading…";
}

- (void)toggleTorch {
	if (self.device == nil || !self.device.hasTorch || ![self.device lockForConfiguration:nil]) {
		return;
	}
	self.device.torchMode = self.device.torchMode == AVCaptureTorchModeOn ? AVCaptureTorchModeOff : AVCaptureTorchModeOn;
	[self.device unlockForConfiguration];
}

- (void)cancel {
	[self finishWithReading:nil error:nil];
}

- (void)choosePhoto {
	if (@available(iOS 14, *)) {
		[self presentPhotoPicker];
	} else {
		self.status.text = @"Choosing a photo needs iOS 14 or later.";
	}
}

- (void)presentPhotoPicker API_AVAILABLE(ios(14)) {
	PHPickerConfiguration *config = [[PHPickerConfiguration alloc] init];
	config.filter = [PHPickerFilter imagesFilter];
	config.selectionLimit = 1;
	PHPickerViewController *picker = [[PHPickerViewController alloc] initWithConfiguration:config];
	picker.delegate = self;
	[self presentViewController:picker animated:YES completion:nil];
}

- (void)picker:(PHPickerViewController *)picker didFinishPicking:(NSArray<PHPickerResult *> *)results API_AVAILABLE(ios(14)) {
	NSItemProvider *item = results.firstObject.itemProvider;
	[picker dismissViewControllerAnimated:YES completion:^{
		if (item == nil) {
			if (self.session == nil) { // opened for a photo only: closing the picker cancels
				[self finishWithReading:nil error:nil];
			}
			return;
		}
		self.status.text = @"Reading the photo…";
		[item loadDataRepresentationForTypeIdentifier:@"public.image" completionHandler:^(NSData *data, NSError *error) {
			NSDictionary *reading = nil;
			NSString *problem = nil;
			if (data == nil) {
				problem = error.localizedDescription ?: @"The photo couldn't be loaded.";
			} else {
				CIImage *image = [CIImage imageWithData:data options:@{kCIImageApplyOrientationProperty : @YES}];
				if (image == nil) {
					problem = @"The photo couldn't be read.";
				} else {
					image = [image imageByApplyingTransform:CGAffineTransformMakeTranslation(-image.extent.origin.x, -image.extent.origin.y)];
					reading = TCGBReadCard(image);
				}
			}
			dispatch_async(dispatch_get_main_queue(), ^{
				[self finishWithReading:reading error:problem];
			});
		}];
	}];
}

// finishWithReading closes the scanner and reports the result (nil and no error:
// cancelled). Called on the main thread.
- (void)finishWithReading:(NSDictionary *)reading error:(NSString *)problem {
	if (self.finished) {
		return;
	}
	self.finished = YES;
	AVCaptureSession *session = self.session;
	if (session != nil) {
		dispatch_async(self.queue, ^{
			[session stopRunning];
		});
	}
	char *json = reading ? TCGBJSONString(reading) : NULL;
	char *err = problem ? strdup(problem.UTF8String) : NULL;
	uintptr_t handle = self.handle;
	[self dismissViewControllerAnimated:YES completion:^{
		tcgbScanDone(handle, json, err, reading == nil && problem == nil);
		free(json);
		free(err);
	}];
}

- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
	return UIInterfaceOrientationMaskPortrait;
}

@end

int tcgb_camera_available(void) {
	// A capture device, not just the camera source type: recent Simulators report the
	// latter without having a camera to open.
	return [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo] != nil ? 1 : 0;
}

void tcgb_scan_start(uintptr_t handle, int photosOnly) {
	dispatch_async(dispatch_get_main_queue(), ^{
		TCGBScanController *vc = [[TCGBScanController alloc] init];
		vc.handle = handle;
		vc.photosOnly = photosOnly || !tcgb_camera_available();
		vc.modalPresentationStyle = UIModalPresentationFullScreen;
		[tcgbTopController() presentViewController:vc animated:YES completion:nil];
	});
}
