program RttiPublishedInvoke;

// Discover and run only published test methods, including bodies reachable only via Invoke.

uses System.Rtti;

type
  TSample = class
  public
    procedure TestPublic;
  published
    procedure TestA;
    procedure SetUp;
  end;

procedure TSample.TestPublic;
begin
  WriteLn('public');
end;

procedure TSample.TestA;
begin
  WriteLn('ran TestA');
end;

procedure TSample.SetUp;
begin
  WriteLn('setup');
end;

var
  Ctx: TRttiContext;
  Method: TRttiMethod;
  Sample: TSample;
begin
  Ctx := TRttiContext.Create;
  Sample := TSample.Create;
  for Method in Ctx.GetType(TSample).GetMethods do
    if (Method.Visibility = TMemberVisibility.mvPublished) and
      (Copy(Method.Name, 1, 4) = 'Test') then
      Method.Invoke(Sample, []);
  Sample.Free;
end.
