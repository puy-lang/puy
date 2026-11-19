unit Aliases;

// Names `Remote`'s class with a top-level alias in its `interface` section.

interface

uses Remote;

type
  TRemoteAlias = TRemoteOuter;

implementation

end.
