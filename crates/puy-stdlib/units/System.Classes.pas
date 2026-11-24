unit System.Classes;

// Streams (docs/spec/stdlib/System.Classes/README.md, TStream.md), the readers and writers on
// them, and the lists of strings. Written from the specification only, not from any other
// implementation.
//
// Split as System.Generics.Collections (docs/spec/collections.md 1.2): Name.Interface.inc holds
// the declarations and Name.Implementation.inc the bodies of one group of types, read only when
// a program names one of them, so each Interface file starts its own `type` section. Streams
// are the streams; Readers the readers and writers; StringList the lists of strings.

interface

uses
  System.SysUtils,
  // The lines of TStringList are a TList<TStringItem>, sorted with a TComparer
  System.Generics.Collections, System.Generics.Defaults,
  // The Rust parts: the handle of an open file (crates/puy-stdlib-ext; docs/design/stdlib.md
  // 3.1), a field of THandleStream
  System.Classes.Native;

{$I System.Classes/Streams.Interface.inc}

{$I System.Classes/Readers.Interface.inc}

{$I System.Classes/StringList.Interface.inc}

implementation

{$I System.Classes/Streams.Implementation.inc}

{$I System.Classes/Readers.Implementation.inc}

{$I System.Classes/StringList.Implementation.inc}

end.
