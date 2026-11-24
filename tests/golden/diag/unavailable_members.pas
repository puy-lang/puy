program UnavailableMembers;

// A member a unit of the standard library lists as not provided (`<class>.<member>` in its
// list; requirements §8.6) is E3022 instead of E0700, on the class and on its descendants, named
// with the class that lists it, also when assigned to or named through the class. Another
// missing member stays E0700, and a class of the program with a member of that name is not
// affected. With System.Classes used, `TList` is the one of System.Classes.

uses
  System.SysUtils, System.Classes;

type
  TMine = class(TMemoryStream)
  end;
  TOther = class
    Memory: Integer;
  end;

var
  L: TList;

begin
  var M := TMemoryStream.Create;
  WriteLn(M.Memory = nil);
  var B := TBytesStream.Create;
  B.memory;
  var X := TMine.Create;
  X.Memory;
  M.NoSuchMember;
  M.Capacity := 10;
  WriteLn(TMemoryStream.Capacity);
  var O := TOther.Create;
  O.Memory := 1;
  O.Free;
end.
