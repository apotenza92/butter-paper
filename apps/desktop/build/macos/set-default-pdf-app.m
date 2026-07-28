#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 2) {
      fprintf(stderr, "Usage: set-default-pdf-app /path/to/Application.app\n");
      return 64;
    }

    NSString *applicationPath = [NSString stringWithUTF8String:argv[1]];
    NSURL *applicationURL = [NSURL fileURLWithPath:applicationPath isDirectory:YES];
    if (![[NSFileManager defaultManager] fileExistsAtPath:applicationURL.path]) {
      fprintf(stderr, "Application bundle not found: %s\n", argv[1]);
      return 66;
    }
    NSString *bundleIdentifier = [NSBundle bundleWithURL:applicationURL].bundleIdentifier;
    if (bundleIdentifier.length == 0) {
      fprintf(stderr, "Application bundle has no bundle identifier: %s\n", argv[1]);
      return 65;
    }

    __block BOOL completed = NO;
    __block NSError *operationError = nil;
    [[NSWorkspace sharedWorkspace]
      setDefaultApplicationAtURL:applicationURL
      toOpenContentType:UTTypePDF
      completionHandler:^(NSError *error) {
        operationError = error;
        completed = YES;
      }];

    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:120.0];
    while (!completed && [deadline timeIntervalSinceNow] > 0) {
      [[NSRunLoop currentRunLoop]
        runMode:NSDefaultRunLoopMode
        beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
    }

    if (!completed) {
      fprintf(stderr, "Timed out waiting for macOS to change the default PDF application.\n");
      return 70;
    }
    if (operationError != nil) {
      fprintf(stderr, "%s\n", operationError.localizedDescription.UTF8String);
      return 1;
    }

    NSURL *defaultApplication = [[NSWorkspace sharedWorkspace]
      URLForApplicationToOpenContentType:UTTypePDF];
    NSString *defaultBundleIdentifier = [NSBundle bundleWithURL:defaultApplication].bundleIdentifier;
    if (defaultApplication == nil
        || ![defaultBundleIdentifier isEqualToString:bundleIdentifier]) {
      fprintf(stderr, "macOS did not retain the selected default PDF application.\n");
      return 1;
    }

    return 0;
  }
}
