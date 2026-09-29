// Small ownership-safe C boundary. All workflow behavior is implemented in Rust.
#import <AppKit/AppKit.h>
#import <EventKit/EventKit.h>
#include <stdlib.h>
#include <string.h>

static NSString *text(NSString *s) { return s ?: @""; }
static NSDictionary *failure(NSString *message) { return @{@"error": text(message)}; }
static NSDictionary *calendarInfo(EKCalendar *c) {
    NSColor *color = [[NSColor colorWithCGColor:c.CGColor] colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    NSString *rgb = color ? [NSString stringWithFormat:@"#%02x%02x%02x", (int)(color.redComponent*255), (int)(color.greenComponent*255), (int)(color.blueComponent*255)] : @"#0a84ff";
    return @{@"id":c.calendarIdentifier, @"title":text(c.title), @"source":text(c.source.title), @"writable":@(c.allowsContentModifications), @"color":rgb};
}
static NSDictionary *eventInfo(EKEvent *e) {
    NSMutableArray *attendees = [NSMutableArray array];
    NSInteger ownStatus = -1;
    for (EKParticipant *p in e.attendees) {
        if (p.isCurrentUser) ownStatus = p.participantStatus;
        [attendees addObject:@{@"name":text(p.name), @"email":text(p.URL.absoluteString), @"status":@(p.participantStatus), @"is_self":@(p.isCurrentUser)}];
    }
    return @{@"id":text(e.eventIdentifier), @"uid":text(e.calendarItemExternalIdentifier), @"item_id":text(e.calendarItemIdentifier),
        @"calendar_id":text(e.calendar.calendarIdentifier), @"title":text(e.title),
        @"start":@((long long)e.startDate.timeIntervalSince1970), @"end":@((long long)e.endDate.timeIntervalSince1970),
        @"all_day":@(e.allDay), @"recurring":@(e.hasRecurrenceRules), @"status":@(e.status), @"self_status":@(ownStatus),
        @"availability":@(e.availability), @"url":text(e.URL.absoluteString), @"location":text(e.location),
        @"notes":text(e.notes), @"attendees":attendees};
}
static NSDictionary *handle(NSDictionary *q) {
    NSString *op = q[@"op"];
    EKAuthorizationStatus status = [EKEventStore authorizationStatusForEntityType:EKEntityTypeEvent];
    if ([op isEqual:@"status"]) return @{@"status":@(status)};
    if ([op isEqual:@"app-installed"]) return @{@"installed":@([NSWorkspace.sharedWorkspace URLForApplicationWithBundleIdentifier:text(q[@"bundle"])] != nil)};
    if ([op isEqual:@"alert"]) {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp activateIgnoringOtherApps:YES];
        NSAlert *alert = [NSAlert new];
        alert.messageText = @"My Schedule";
        alert.informativeText = text(q[@"message"]);
        [alert addButtonWithTitle:@"OK"];
        [alert runModal];
        return @{@"ok":@YES};
    }
    EKEventStore *store = [EKEventStore new];
    if ([op isEqual:@"authorize"]) {
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        __block BOOL granted = NO;
        __block NSString *message = nil;
        [store requestFullAccessToEventsWithCompletion:^(BOOL allowed, NSError *error) {
            granted = allowed;
            message = error.localizedDescription;
            dispatch_semaphore_signal(done);
        }];
        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:120];
        while (dispatch_semaphore_wait(done, DISPATCH_TIME_NOW) != 0) {
            if (deadline.timeIntervalSinceNow <= 0) return failure(@"Calendar permission is still pending. Allow Full Access in System Settings, then open schedule again.");
            [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }
        return granted ? @{@"ok":@YES} : failure(message ?: @"Enable Full Access to Calendars for Alfred / My Schedule in System Settings → Privacy & Security → Calendars.");
    }
    if (status != EKAuthorizationStatusFullAccess) return failure(@"Calendar access is required. Open schedule and choose Allow calendar access.");
    if ([op isEqual:@"snapshot"]) {
        NSDate *start = [NSDate dateWithTimeIntervalSince1970:[q[@"start"] doubleValue]];
        NSDate *end = [NSDate dateWithTimeIntervalSince1970:[q[@"end"] doubleValue]];
        if ([end timeIntervalSinceDate:start] <= 0 || [end timeIntervalSinceDate:start] > 367*86400) return failure(@"Invalid calendar date range.");
        NSMutableArray *calendars = [NSMutableArray array];
        for (EKCalendar *c in [store calendarsForEntityType:EKEntityTypeEvent]) [calendars addObject:calendarInfo(c)];
        NSPredicate *predicate = [store predicateForEventsWithStartDate:start endDate:end calendars:nil];
        NSMutableArray *events = [NSMutableArray array];
        for (EKEvent *e in [store eventsMatchingPredicate:predicate]) {
            if (e.startDate && e.endDate) [events addObject:eventInfo(e)];
        }
        return @{@"calendars":calendars, @"events":events, @"default_calendar":text(store.defaultCalendarForNewEvents.calendarIdentifier)};
    }
    if ([op isEqual:@"create"]) {
        EKCalendar *calendar = [store calendarWithIdentifier:q[@"calendar_id"]];
        if (!calendar || !calendar.allowsContentModifications) return failure(@"That calendar is no longer writable. Choose another calendar.");
        double start = [q[@"start"] doubleValue], end = [q[@"end"] doubleValue];
        NSString *title = q[@"title"];
        if (!title.length || title.length > 500 || end <= start || end - start > 7*86400) return failure(@"Invalid event title or duration.");
        EKEvent *event = [EKEvent eventWithEventStore:store];
        event.calendar = calendar;
        event.title = title;
        event.startDate = [NSDate dateWithTimeIntervalSince1970:start];
        event.endDate = [NSDate dateWithTimeIntervalSince1970:end];
        event.timeZone = NSTimeZone.localTimeZone;
        event.location = text(q[@"location"]);
        event.notes = text(q[@"notes"]);
        NSString *url = text(q[@"url"]);
        if (url.length) event.URL = [NSURL URLWithString:url];
        NSInteger minutes = [q[@"reminder"] integerValue];
        if (minutes >= 0) [event addAlarm:[EKAlarm alarmWithRelativeOffset:-minutes*60]];
        NSError *error = nil;
        if (![store saveEvent:event span:EKSpanThisEvent commit:YES error:&error]) return failure(error.localizedDescription);
        return @{@"event":eventInfo(event)};
    }
    return failure(@"Unknown EventKit operation.");
}
char *schedule_call(const char *input) {
    @autoreleasepool {
        @try {
            NSData *data = [[NSString stringWithUTF8String:input] dataUsingEncoding:NSUTF8StringEncoding];
            id q = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
            NSDictionary *answer = [q isKindOfClass:NSDictionary.class] ? handle(q) : failure(@"Invalid request.");
            NSData *output = [NSJSONSerialization dataWithJSONObject:answer options:0 error:nil];
            NSString *json = [[NSString alloc] initWithData:output encoding:NSUTF8StringEncoding];
            return strdup(json.UTF8String ?: "{\"error\":\"EventKit serialization failed\"}");
        } @catch (NSException *exception) {
            NSData *output = [NSJSONSerialization dataWithJSONObject:failure(exception.reason) options:0 error:nil];
            return strdup([[NSString alloc] initWithData:output encoding:NSUTF8StringEncoding].UTF8String);
        }
    }
}
void schedule_free(char *value) { free(value); }
