program RttiPublishedNoM;

// Check how explicit public and published methods are exposed without an M directive.

uses System.TypInfo, System.Rtti;

type
  TProbe = class(TObject)
  public
    procedure TestPublic;
  published
    procedure TestA;
  end;

procedure TProbe.TestPublic;
begin
end;

procedure TProbe.TestA;
begin
end;

var
  Ctx: TRttiContext;
  Method: TRttiMethod;
  PublicVisibility, PublishedVisibility: Integer;
begin
  Ctx := TRttiContext.Create;
  PublicVisibility := -1;
  PublishedVisibility := -1;
  for Method in Ctx.GetType(TProbe).GetMethods do
  begin
    if Method.Name = 'TestPublic' then
      PublicVisibility := Ord(Method.Visibility);
    if Method.Name = 'TestA' then
      PublishedVisibility := Ord(Method.Visibility);
  end;
  WriteLn('TestPublic ', PublicVisibility);
  WriteLn('TestA ', PublishedVisibility);
end.
