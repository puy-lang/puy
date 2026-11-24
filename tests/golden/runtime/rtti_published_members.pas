program RttiPublishedMembers;

// Check the visibility ordinals of public and published fields, properties and methods.

uses System.Rtti;

type
  TSample = class
    DefaultField: Integer;
  public
    PublicField: Integer;
    procedure PublicMethod;
    property PublicProperty: Integer read PublicField;
  published
    PublishedField: Integer;
    procedure PublishedMethod;
    property PublishedProperty: Integer read PublishedField;
  end;

procedure TSample.PublicMethod;
begin
end;

procedure TSample.PublishedMethod;
begin
end;

var
  Ctx: TRttiContext;
  Info: TRttiType;
begin
  Ctx := TRttiContext.Create;
  Info := Ctx.GetType(TSample);
  WriteLn(Ord(Info.GetField('DefaultField').Visibility));
  WriteLn(Ord(Info.GetField('PublicField').Visibility), ' ',
    Ord(Info.GetProperty('PublicProperty').Visibility), ' ',
    Ord(Info.GetMethod('PublicMethod').Visibility));
  WriteLn(Ord(Info.GetField('PublishedField').Visibility), ' ',
    Ord(Info.GetProperty('PublishedProperty').Visibility), ' ',
    Ord(Info.GetMethod('PublishedMethod').Visibility));
end.
