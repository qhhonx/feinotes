// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
#import <Foundation/Foundation.h>
#include <unistd.h>

int main(int argc, char **argv) {
    @autoreleasepool {
        NSBundle *bundle = NSBundle.mainBundle;
        if (![bundle.bundleIdentifier isEqualToString:@"app.quietmd.cloudlab"]) return 2;
        NSString *channel = bundle.infoDictionary[@"FeiUpstreamPreviewCloudChannel"];
        if (![[[NSUUID alloc] initWithUUIDString:channel] UUIDString]) return 2;
        NSString *root = [NSHomeDirectory() stringByAppendingPathComponent:
            [@"tmp/quiet-sync-qa-upstream-cloud-" stringByAppendingString:channel]];
        if (![root containsString:@"/Library/Containers/app.quietmd.cloudlab/Data/tmp/"]) return 2;
        NSError *error = nil;
        if (![NSFileManager.defaultManager createDirectoryAtPath:root withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0700} error:&error]) {
            fprintf(stderr,"Preview library: %s\n",error.localizedDescription.UTF8String); return 2;
        }
        for (NSString *key in NSProcessInfo.processInfo.environment) {
            if ([key hasPrefix:@"QUIET_"] || [key hasPrefix:@"FDL_"] || [key hasPrefix:@"DYLD_"]) unsetenv(key.UTF8String);
        }
        setenv("QUIET_SYNC_QA_ROOT",root.UTF8String,1);
        setenv("QUIET_SYNC_QA_CHANNEL",channel.UTF8String,1);
        setenv("QUIET_SYNC_QA_MIGRATION","1",1);
        setenv("QUIET_SYNC_QA_LOCAL_FIRST","1",1);
        setenv("QUIET_MD_LOG_DIR",[root stringByAppendingPathComponent:@"logs"].UTF8String,1);
        int first = 1;
        if (argc > 2 && !strcmp(argv[1],"--qa-script")) {
            NSString *name = [NSString stringWithUTF8String:argv[2]];
            if (![name.lastPathComponent isEqualToString:name] || [name hasPrefix:@"."]) return 2;
            NSString *script = [root stringByAppendingPathComponent:name];
            if (![[script stringByResolvingSymlinksInPath] isEqualToString:script]) return 2;
            setenv("FDL_SCRIPT",script.UTF8String,1);
            setenv("FDL_SHOTS",root.UTF8String,1);
            setenv("FDL_NOBLINK","1",1);
            setenv("QUIET_SYNC_QA_STARTUP","1",1);
            first = 3;
        }
        NSString *real = [bundle.executablePath.stringByDeletingLastPathComponent stringByAppendingPathComponent:@"quiet_md.bin"];
        char **arguments = calloc((size_t)argc + 1,sizeof(char *));
        arguments[0] = (char *)real.UTF8String;
        for (int i=first;i<argc;i++) arguments[i-first+1] = argv[i];
        execv(real.UTF8String,arguments);
        perror("Preview executable"); return 2;
    }
}
