program Distinct;

uses
  System.SysUtils, System.Generics.Collections;

// Distinct types (`type X = type Y`): the same values and operations as Y, but a type of their
// own for overloads and helpers. Here THtml is text that is already escaped, so Render accepts
// a THtml as it is and escapes a plain string; TMeters and TFeet get helpers of their own; and
// TUserId and TOrderId choose different overloads although both are Int64. Values stay
// assignment compatible with the root type, and arithmetic yields the root type.

type
  THtml = type string;
  TMeters = type Double;
  TFeet = type Double;
  TUserId = type Int64;
  TOrderId = type Int64;

  TMetersHelper = record helper for TMeters
    function ToFeet: TFeet;
    function Describe: string;
  end;

  TFeetHelper = record helper for TFeet
    function ToMeters: TMeters;
    function Describe: string;
  end;

function TMetersHelper.ToFeet: TFeet;
begin
  Result := Self / 0.3048;
end;

function TMetersHelper.Describe: string;
begin
  Result := Format('%.1f m', [Double(Self)]);
end;

function TFeetHelper.ToMeters: TMeters;
begin
  Result := Self * 0.3048;
end;

function TFeetHelper.Describe: string;
begin
  Result := Format('%.1f ft', [Double(Self)]);
end;

function Escape(const S: string): THtml;
begin
  Result := '';
  for var C in S do
    case C of
      '<': Result := Result + '&lt;';
      '>': Result := Result + '&gt;';
      '&': Result := Result + '&amp;';
    else
      Result := Result + C;
    end;
end;

// Already escaped: written as it is.
procedure Render(const Html: THtml); overload;
begin
  WriteLn('  ', Html);
end;

// Plain text: escaped first.
procedure Render(const Text: string); overload;
begin
  Render(Escape(Text));
end;

function Bold(const Html: THtml): THtml;
begin
  Result := '<b>' + Html + '</b>';
end;

procedure Describe(Id: TUserId); overload;
begin
  WriteLn('  user #', Id);
end;

procedure Describe(Id: TOrderId); overload;
begin
  WriteLn('  order #', Id);
end;

var
  Owners: TDictionary<TOrderId, TUserId>;
  Height: TMeters;
  Ceiling: TFeet;

begin
  WriteLn('THtml and string:');
  Render('if a < b & b > c');
  Render(Bold(Escape('Tom & Jerry')));
  // A string literal converts to THtml: markup written in the source is trusted.
  Render(Bold('already <i>marked up</i>'));

  WriteLn('TMeters and TFeet:');
  Height := 1.8;
  Ceiling := 9;
  WriteLn('  ', Height.Describe, ' = ', Height.ToFeet.Describe);
  WriteLn('  ', Ceiling.Describe, ' = ', Ceiling.ToMeters.Describe);
  // Height * 2 is a Double (the root type); converting it back makes it TMeters again.
  WriteLn('  twice the height: ', TMeters(Height * 2).Describe);

  WriteLn('TUserId and TOrderId:');
  Owners := TDictionary<TOrderId, TUserId>.Create;
  try
    Owners.Add(TOrderId(5001), TUserId(42));
    Owners.Add(TOrderId(5002), TUserId(7));
    for var Pair in Owners do
    begin
      Describe(Pair.Key);
      Describe(Pair.Value);
    end;
    var Plain: Int64 := Owners[5001];
    WriteLn('  as a plain Int64: ', Plain + 1);
  finally
    Owners.Free;
  end;
end.
