import StoreKit
import XCTest

@testable import OpenPdfEdit

/// Guards the tests that need the local product catalogue.
///
/// The StoreKit test service in some simulator runtimes — iOS 26.5, as of
/// writing — accepts a configuration and then answers product requests from
/// the real Media API anyway, leaving an empty catalogue. It is visible in
/// `storekitd`'s own log: the configuration is saved, and the request goes
/// out to the network regardless. The same tests pass on iOS 17.0, which is
/// also this app's deployment target, so `scripts/test.sh` pins that.
///
/// This exists for the runners that cannot: a CI machine has whatever
/// simulator runtime its image shipped with, and a red build there would
/// say nothing about this app.
///
/// The skip is narrow on purpose. It fires only when a catalogue that
/// definitely contains products yields none — a condition no change to this
/// app can produce, since nothing here decides what the test service
/// answers. A regression in `Store` shows up as a failure, not a skip.
func requireLocalStoreKit(_ products: [Product]) throws {
    try XCTSkipIf(
        products.isEmpty,
        """
        This simulator's StoreKit test service ignored the local product \
        catalogue. Run apps/ios/scripts/test.sh, which pins an iOS 17 \
        simulator, to exercise the purchase tests.
        """
    )
}
