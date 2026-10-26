#import "micro_audio.h"

#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>
#include <dispatch/dispatch.h>

/* Foreground players are independent, so several URL sounds mix naturally.
 * They are keyed by the handle the Rust host allocated; the end notification
 * removes finished one-shots instead of leaking a player per sound. */
static NSMutableDictionary<NSNumber *, AVPlayer *> *g_players;
static NSMutableDictionary<NSNumber *, id> *g_observers;
static AVQueuePlayer *g_background_player;
static AVPlayerLooper *g_background_looper;

/* AVFoundation is happy to create and control players off the main thread, so
 * a private serial queue keeps audio independent of the UI run loop. This
 * matters for tests and for host calls made from a worker thread: waiting on
 * the main queue there can deadlock because the main thread may be parked. */
static dispatch_queue_t g_audio_queue;
static void *g_audio_queue_key = &g_audio_queue_key;

static void micro_audio_prepare(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        g_audio_queue = dispatch_queue_create("dev.micros.audio", DISPATCH_QUEUE_SERIAL);
        dispatch_queue_set_specific(g_audio_queue, g_audio_queue_key, g_audio_queue_key, NULL);
        g_players = [NSMutableDictionary dictionary];
        g_observers = [NSMutableDictionary dictionary];
    });
}

static void micro_audio_run(dispatch_block_t block) {
    micro_audio_prepare();
    if (dispatch_get_specific(g_audio_queue_key) == g_audio_queue_key) {
        block();
    } else {
        dispatch_sync(g_audio_queue, block);
    }
}

/* The Rust layer already rejects non-HTTP(S) URLs; repeat the check at the
 * bridge so a C caller cannot turn the player into a local-file reader. */
static NSURL *micro_audio_url(NSString *text) {
    if (text.length == 0 || text.length > 4096) {
        return nil;
    }
    NSURL *url = [NSURL URLWithString:text];
    NSString *scheme = url.scheme.lowercaseString;
    if (![scheme isEqualToString:@"http"] && ![scheme isEqualToString:@"https"]) {
        return nil;
    }
    return url;
}

static AVPlayer *micro_audio_make_player(NSURL *url, BOOL loop, AVPlayerLooper **looper_out) {
    AVPlayerItem *item = [AVPlayerItem playerItemWithURL:url];
    if (loop) {
        AVQueuePlayer *queue = [AVQueuePlayer queuePlayerWithItems:@[item]];
        AVPlayerLooper *looper = [AVPlayerLooper playerLooperWithPlayer:queue templateItem:item];
        if (looper_out) {
            *looper_out = looper;
        }
        return queue;
    }
    return [AVPlayer playerWithPlayerItem:item];
}

int32_t micro_audio_play_url(const char *url_text, int32_t handle) {
    if (!url_text || handle <= 0) {
        return 0;
    }
    NSString *text = [NSString stringWithUTF8String:url_text];
    NSURL *url = micro_audio_url(text);
    if (!url) {
        return 0;
    }

    __block int32_t created = 0;
    micro_audio_run(^{
        micro_audio_prepare();
        if (g_players[@(handle)]) {
            return;
        }
        AVPlayer *player = micro_audio_make_player(url, NO, NULL);
        if (!player) {
            return;
        }
        g_players[@(handle)] = player;
        AVPlayerItem *item = player.currentItem;
        if (item) {
            __weak AVPlayer *weak_player = player;
            id token = [[NSNotificationCenter defaultCenter]
                addObserverForName:AVPlayerItemDidPlayToEndTimeNotification
                            object:item
                             queue:nil
                        usingBlock:^(NSNotification *note) {
                            micro_audio_run(^{
                                [weak_player pause];
                                id observer = g_observers[@(handle)];
                                if (observer) {
                                    [[NSNotificationCenter defaultCenter] removeObserver:observer];
                                    [g_observers removeObjectForKey:@(handle)];
                                }
                                [g_players removeObjectForKey:@(handle)];
                            });
                        }];
            g_observers[@(handle)] = token;
        }
        [player play];
        created = 1;
    });
    return created;
}

void micro_audio_stop_url(int32_t handle) {
    if (handle <= 0) {
        return;
    }
    micro_audio_run(^{
        micro_audio_prepare();
        id observer = g_observers[@(handle)];
        if (observer) {
            [[NSNotificationCenter defaultCenter] removeObserver:observer];
            [g_observers removeObjectForKey:@(handle)];
        }
        AVPlayer *player = g_players[@(handle)];
        if (!player) {
            return;
        }
        [player pause];
        [g_players removeObjectForKey:@(handle)];
    });
}

void micro_audio_stop_all_urls(void) {
    micro_audio_run(^{
        micro_audio_prepare();
        for (id observer in g_observers.allValues) {
            [[NSNotificationCenter defaultCenter] removeObserver:observer];
        }
        [g_observers removeAllObjects];
        for (AVPlayer *player in g_players.allValues) {
            [player pause];
        }
        [g_players removeAllObjects];
    });
}

void micro_audio_play_background(const char *url_text) {
    if (!url_text) {
        return;
    }
    NSString *text = [NSString stringWithUTF8String:url_text];
    NSURL *url = micro_audio_url(text);
    if (!url) {
        return;
    }

    micro_audio_run(^{
        micro_audio_prepare();
        [g_background_player pause];
        g_background_player = nil;
        g_background_looper = nil;

        AVPlayerLooper *looper = nil;
        AVQueuePlayer *player = micro_audio_make_player(url, YES, &looper);
        if (!player) {
            return;
        }
        g_background_player = player;
        g_background_looper = looper;
        [player play];
    });
}

void micro_audio_stop_background(void) {
    micro_audio_run(^{
        [g_background_player pause];
        g_background_player = nil;
        g_background_looper = nil;
    });
}
