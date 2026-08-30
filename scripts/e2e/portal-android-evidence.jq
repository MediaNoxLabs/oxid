def measured($mode; $name):
  (first($scenarios[] | select(.mode == $mode) | .measurements[$name]) // false);
{
  schema:"oxid-portal-android-evidence-v1",
  oxid:{head:$head},
  portal:{
    integrationCommit:$commit,
    integrationTree:$tree,
    images:{resolver:$resolver,didManager:$didManager,issuer:$issuer}
  },
  platform:{
    kind:"android_physical_tailnet",
    os:$os,
    apiLevel:$api,
    applicationId:"io.medianox.oxid"
  },
  measurements:{
    completedSeconds:$duration,
    portalConsumerCleanup:$portalConsumerCleanup,
    protocolCounters:$counters,
    scenarioResults:$scenarios
  },
  acceptance:{
    mockKycApproved:($counters.kyc == 14),
    warmIngress:([
      "route-refuse",
      "malformed",
      "protocol-error",
      "protocol-timeout",
      "issue-error",
      "issue"
    ] | all(.[]; measured(.; "warmIngress") == true)),
    coldIngress:measured("cold-route"; "coldIngress"),
    refusalBeforeConsent:measured("route-refuse"; "refusalBeforeConsent"),
    refusalSecretEndpointCalls:measured("route-refuse"; "refusalSecretEndpointCalls"),
    malformedRejected:measured("malformed"; "malformedRejected"),
    unavailableRejected:measured("protocol-error"; "unavailableRejected"),
    timeoutRejected:measured("protocol-timeout"; "timeoutRejected"),
    issueErrorEscapedSafely:measured("issue-error"; "issueErrorEscapedSafely"),
    exactProtocolCounters:(
      $counters.authorizationMetadata == 3
      and $counters.credential == 1
      and $counters.issuerMetadata == 6
      and $counters.issuerResolution == 3
      and $counters.issuerResolutionSuccess == 3
      and $counters.kyc == 14
      and $counters.nonce == 1
      and $counters.other == 0
      and $counters.token == 2
    ),
    strictFinalExchange:measured("issue"; "strictFinalExchange"),
    explicitConsent:measured("issue"; "explicitConsent"),
    managedAuthenticationProof:measured("issue"; "managedAuthenticationProof"),
    separateJubjubAssertionBinding:measured("issue"; "separateJubjubAssertionBinding"),
    exactBundleImported:measured("issue"; "exactBundleImported"),
    encryptedPersistence:$encryptedPersistence,
    processRestart:$processRestart,
    custodyReactivated:measured("restored"; "custodyReactivated"),
    listedAfterRestart:measured("restored"; "listedAfterRestart"),
    freshReverification:measured("restored"; "freshReverification"),
    oneItemIngress:measured("cold-route"; "oneItemIngress"),
    noAdbReverse:$noAdbReverse,
    tailnetIdentityDiscovered:$tailnetIdentityDiscovered,
    temporaryListenerDiscovered:$temporaryListenerDiscovered,
    preservedStandaloneRoutes:$preservedStandaloneRoutes,
    exactServeReceiptCleanup:$exactServeReceiptCleanup,
    portalConsumerCleanup:$portalConsumerCleanup,
    completedWithin300Seconds:($duration <= 300)
  }
}
