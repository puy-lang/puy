program DeliverySlots;

uses
  System.SysUtils, System.DateUtils, System.Math, Puy.Lazy, Puy.Random;

// Calculate how many slots the parcels need and shuffle them with a repeatable seed.
function MakeSlots: TArray<Integer>;
begin
  const Parcels = 11;
  const PerSlot = 4;
  var SlotCount := Ceil(Parcels / PerSlot);
  SetLength(Result, SlotCount);
  for var Index := 0 to High(Result) do
    Result[Index] := Index;
  var Random := TRandom.Create(42);
  Random.Shuffle<Integer>(Result);
end;

begin
  var Slots := TLazy<TArray<Integer>>.Create(MakeSlots);
  var Opening := TInstant.Parse('2026-06-01T09:00:00Z');
  var SlotLength := TTimeSpan.FromMinutes(30);
  var LocalBooking := EncodeDateTime(2026, 6, 1, 9, 0, 0, 0);
  WriteLn('Booking day: ', FormatDateTime('yyyy-mm-dd', DateOf(LocalBooking)));
  WriteLn('Slots prepared: ', Slots.IsValueCreated);
  for var Slot in Slots.Value do
  begin
    var StartAt := Opening + TTimeSpan.FromMinutes(Slot * SlotLength.TotalMinutes);
    WriteLn('Slot ', Slot + 1, ': ', StartAt.ToString, ' to ', (StartAt + SlotLength).ToString);
  end;
  WriteLn('Slots prepared: ', Slots.IsValueCreated, ', count: ', Length(Slots.Value));
end.
