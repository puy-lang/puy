unit System.Generics.Collections;

// The containers (docs/spec/collections.md). Written from the specification only, not from
// any other implementation.
//
// Both sections are split into one included file per container, in the same order, so that
// the containers can be filled in separately (docs/spec/collections.md 1.2):
// Name.Interface.inc holds the declarations, Name.Implementation.inc the bodies. Keep each
// container's declarations and bodies inside its own files; anything shared by several
// containers goes in the Common files.
// The build of puy-stdlib puts the files in place of the {$I} lines; diagnostics and stack
// traces show the included file and its own line numbers. The files of a container are read
// only when a program names it (docs/spec/collections.md 1.2), so each Interface file starts its
// own `type` section.

interface

uses
  System.SysUtils, System.Generics.Defaults;

{$I System.Generics.Collections/Common.Interface.inc}

{$I System.Generics.Collections/List.Interface.inc}

{$I System.Generics.Collections/StackQueue.Interface.inc}

{$I System.Generics.Collections/Dictionary.Interface.inc}

{$I System.Generics.Collections/HashSet.Interface.inc}

{$I System.Generics.Collections/Owning.Interface.inc}

{$I System.Generics.Collections/Array.Interface.inc}

implementation

{$I System.Generics.Collections/Common.Implementation.inc}

{$I System.Generics.Collections/List.Implementation.inc}

{$I System.Generics.Collections/StackQueue.Implementation.inc}

{$I System.Generics.Collections/Dictionary.Implementation.inc}

{$I System.Generics.Collections/HashSet.Implementation.inc}

{$I System.Generics.Collections/Owning.Implementation.inc}

{$I System.Generics.Collections/Array.Implementation.inc}

end.
