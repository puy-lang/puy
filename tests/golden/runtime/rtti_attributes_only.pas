program RttiAttributesOnly;

// A program reading the attributes and no other member still reads them: the API returning
// the attributes gives the types their member stage (rtti.md 1.3, 1.5.1).

uses System.Rtti;

type
  TTagAttribute = class(TCustomAttribute)
  end;

  [TTag]
  TWorker = class
  end;

begin
  WriteLn(Length(TRttiContext.Create.GetType(TWorker).GetAttributes));
end.
